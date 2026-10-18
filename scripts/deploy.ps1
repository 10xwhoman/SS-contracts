<#
.SYNOPSIS
    StellarSettle – Contract Deployment Script (PowerShell)

.DESCRIPTION
    Deploys and initialises the following Soroban contracts:
      1. invoice-token      (SEP-41 invoice tokenisation)
      2. invoice-escrow     (escrow lifecycle + settlement)
      3. payment-distributor (settlement/refund payout fan-out)

.EXAMPLE
    # 1. Copy .env.example to .env and fill in your values
    Copy-Item .env.example .env
    notepad .env          # or your editor of choice

    # 2. Run from the repo root (allow script execution if needed)
    Set-ExecutionPolicy -Scope Process -ExecutionPolicy Bypass
    .\scripts\deploy.ps1

.NOTES
    Requirements:
      - soroban-cli  (cargo install --locked soroban-cli --features opt)
      - Contracts already built:  soroban contract build
    Environment variables are loaded from .env (if present) or the current
    shell environment.  See .env.example for the full list.
#>

#Requires -Version 5.1
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

# ---------------------------------------------------------------------------
# Colour helpers
# ---------------------------------------------------------------------------
function Write-Info    { param([string]$Msg) Write-Host "[INFO]  $Msg" -ForegroundColor Cyan    }
function Write-Ok      { param([string]$Msg) Write-Host "[OK]    $Msg" -ForegroundColor Green   }
function Write-Warn    { param([string]$Msg) Write-Host "[WARN]  $Msg" -ForegroundColor Yellow  }
function Write-Err     { param([string]$Msg) Write-Host "[ERROR] $Msg" -ForegroundColor Red; exit 1 }

# ---------------------------------------------------------------------------
# Locate the repo root (parent of the 'scripts' folder)
# ---------------------------------------------------------------------------
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$RepoRoot  = Split-Path -Parent $ScriptDir

# ---------------------------------------------------------------------------
# Load .env (if present)
# ---------------------------------------------------------------------------
$EnvFile = Join-Path $RepoRoot '.env'
if (Test-Path $EnvFile) {
    Write-Info "Loading environment from $EnvFile"
    Get-Content $EnvFile | ForEach-Object {
        $line = $_.Trim()
        # Skip comments and blank lines
        if ($line -and -not $line.StartsWith('#')) {
            $eqIdx = $line.IndexOf('=')
            if ($eqIdx -gt 0) {
                $key   = $line.Substring(0, $eqIdx).Trim()
                $value = $line.Substring($eqIdx + 1).Trim().Trim('"').Trim("'")
                [System.Environment]::SetEnvironmentVariable($key, $value, 'Process')
            }
        }
    }
} else {
    Write-Warn '.env file not found – using environment variables already set in shell'
}

# ---------------------------------------------------------------------------
# Read variables (with defaults where sensible)
# ---------------------------------------------------------------------------
function Require-Env {
    param([string]$Name, [string]$Description)
    $val = [System.Environment]::GetEnvironmentVariable($Name)
    if ([string]::IsNullOrWhiteSpace($val)) {
        Write-Err "Required environment variable '$Name' is not set. $Description"
    }
    return $val
}

function Get-EnvOrDefault {
    param([string]$Name, [string]$Default)
    $val = [System.Environment]::GetEnvironmentVariable($Name)
    if ([string]::IsNullOrWhiteSpace($val)) { return $Default }
    return $val
}

$StellarNetwork            = Require-Env 'STELLAR_NETWORK'            'Use testnet, mainnet, or futurenet'
$StellarSecretKey          = Require-Env 'STELLAR_SECRET_KEY'         'Deployer secret key (starts with S)'
$AdminPublicKey            = Require-Env 'ADMIN_PUBLIC_KEY'           'Admin public key (starts with G)'
$PlatformFeeBps            = Require-Env 'PLATFORM_FEE_BPS'           'Fee basis points (e.g. 300 = 3%)'
$InvoiceTokenName          = Require-Env 'INVOICE_TOKEN_NAME'         'Human-readable token name'
$InvoiceTokenSymbol        = Require-Env 'INVOICE_TOKEN_SYMBOL'       'Ticker symbol (≤12 chars)'
$InvoiceTokenDecimals      = Require-Env 'INVOICE_TOKEN_DECIMALS'     'Decimal places (e.g. 7)'
$InvoiceTokenInvoiceId     = Require-Env 'INVOICE_TOKEN_INVOICE_ID'   'Invoice identifier (Soroban Symbol)'

$WasmInvoiceEscrow         = Get-EnvOrDefault 'WASM_INVOICE_ESCROW'         'target/wasm32v1-none/release/invoice_escrow.wasm'
$WasmInvoiceToken          = Get-EnvOrDefault 'WASM_INVOICE_TOKEN'          'target/wasm32v1-none/release/invoice_token.wasm'
$WasmPaymentDistributor    = Get-EnvOrDefault 'WASM_PAYMENT_DISTRIBUTOR'    'target/wasm32v1-none/release/payment_distributor.wasm'

$ExistingEscrowId           = Get-EnvOrDefault 'INVOICE_ESCROW_CONTRACT_ID'     ''
$ExistingTokenId            = Get-EnvOrDefault 'INVOICE_TOKEN_CONTRACT_ID'      ''
$ExistingDistributorId      = Get-EnvOrDefault 'PAYMENT_DISTRIBUTOR_CONTRACT_ID' ''

# ---------------------------------------------------------------------------
# Verify WASM files exist
# ---------------------------------------------------------------------------
Push-Location $RepoRoot

foreach ($wasm in @($WasmInvoiceEscrow, $WasmInvoiceToken, $WasmPaymentDistributor)) {
    if (-not (Test-Path $wasm)) {
        Write-Err "WASM not found: $wasm`n       Run 'soroban contract build' first."
    }
}

# ---------------------------------------------------------------------------
# Helper: deploy one contract
# ---------------------------------------------------------------------------
function Deploy-Contract {
    param(
        [string]$Label,
        [string]$WasmPath,
        [string]$ExistingId
    )

    if (-not [string]::IsNullOrWhiteSpace($ExistingId)) {
        Write-Warn "$Label`: Skipping deploy – reusing existing ID: $ExistingId"
        return $ExistingId
    }

    Write-Info "Deploying $Label from $WasmPath ..."
    $id = soroban contract deploy `
        --source $StellarSecretKey `
        --network $StellarNetwork `
        --wasm $WasmPath

    if ($LASTEXITCODE -ne 0) {
        Write-Err "Deploy failed for $Label (exit code $LASTEXITCODE)"
    }

    Write-Ok "$Label deployed → $id"
    return $id.Trim()
}

# ---------------------------------------------------------------------------
# 1. Deploy invoice-token
# ---------------------------------------------------------------------------
Write-Host ""
Write-Host "════════════════════════════════════════════════════════" -ForegroundColor Magenta
Write-Host "  Step 1 / 3  –  invoice-token"                          -ForegroundColor Magenta
Write-Host "════════════════════════════════════════════════════════" -ForegroundColor Magenta

$InvoiceTokenId = Deploy-Contract 'invoice-token' $WasmInvoiceToken $ExistingTokenId

# ---------------------------------------------------------------------------
# 2. Deploy invoice-escrow
# ---------------------------------------------------------------------------
Write-Host ""
Write-Host "════════════════════════════════════════════════════════" -ForegroundColor Magenta
Write-Host "  Step 2 / 3  –  invoice-escrow"                         -ForegroundColor Magenta
Write-Host "════════════════════════════════════════════════════════" -ForegroundColor Magenta

$InvoiceEscrowId = Deploy-Contract 'invoice-escrow' $WasmInvoiceEscrow $ExistingEscrowId

# ---------------------------------------------------------------------------
# 3. Deploy payment-distributor
# ---------------------------------------------------------------------------
Write-Host ""
Write-Host "════════════════════════════════════════════════════════" -ForegroundColor Magenta
Write-Host "  Step 3 / 3  –  payment-distributor"                    -ForegroundColor Magenta
Write-Host "════════════════════════════════════════════════════════" -ForegroundColor Magenta

$PaymentDistributorId = Deploy-Contract 'payment-distributor' $WasmPaymentDistributor $ExistingDistributorId

# ---------------------------------------------------------------------------
# Initialise contracts
# ---------------------------------------------------------------------------
Write-Host ""
Write-Host "════════════════════════════════════════════════════════" -ForegroundColor Magenta
Write-Host "  Initialising contracts"                                 -ForegroundColor Magenta
Write-Host "════════════════════════════════════════════════════════" -ForegroundColor Magenta

# --- invoice-token.initialize ---
Write-Info "Initialising invoice-token ..."
Write-Info "  admin        = $AdminPublicKey"
Write-Info "  name         = $InvoiceTokenName"
Write-Info "  symbol       = $InvoiceTokenSymbol"
Write-Info "  decimals     = $InvoiceTokenDecimals"
Write-Info "  invoice_id   = $InvoiceTokenInvoiceId"
Write-Info "  minter       = $InvoiceEscrowId  (escrow contract)"

soroban contract invoke `
    --source $StellarSecretKey `
    --network $StellarNetwork `
    --id $InvoiceTokenId `
    -- initialize `
    --admin      $AdminPublicKey `
    --name       $InvoiceTokenName `
    --symbol     $InvoiceTokenSymbol `
    --decimals   $InvoiceTokenDecimals `
    --invoice_id $InvoiceTokenInvoiceId `
    --minter     $InvoiceEscrowId

if ($LASTEXITCODE -ne 0) { Write-Err "invoice-token initialization failed" }
Write-Ok "invoice-token initialised"

# --- invoice-escrow.initialize ---
Write-Info "Initialising invoice-escrow ..."
Write-Info "  admin            = $AdminPublicKey"
Write-Info "  platform_fee_bps = $PlatformFeeBps"

soroban contract invoke `
    --source $StellarSecretKey `
    --network $StellarNetwork `
    --id $InvoiceEscrowId `
    -- initialize `
    --admin            $AdminPublicKey `
    --platform_fee_bps $PlatformFeeBps

if ($LASTEXITCODE -ne 0) { Write-Err "invoice-escrow initialization failed" }
Write-Ok "invoice-escrow initialised"

# --- payment-distributor.initialize ---
Write-Info "Initialising payment-distributor ..."
Write-Info "  admin = $AdminPublicKey"

soroban contract invoke `
    --source $StellarSecretKey `
    --network $StellarNetwork `
    --id $PaymentDistributorId `
    -- initialize `
    --admin $AdminPublicKey

if ($LASTEXITCODE -ne 0) { Write-Err "payment-distributor initialization failed" }
Write-Ok "payment-distributor initialised"

# --- invoice-escrow.set_payment_distributor ---
Write-Info "Wiring invoice-escrow to payment-distributor ..."

soroban contract invoke `
    --source $StellarSecretKey `
    --network $StellarNetwork `
    --id $InvoiceEscrowId `
    -- set_payment_distributor `
    --payment_distributor $PaymentDistributorId

if ($LASTEXITCODE -ne 0) { Write-Err "invoice-escrow distributor wiring failed" }
Write-Ok "invoice-escrow wired to payment-distributor"

# ---------------------------------------------------------------------------
# Summary
# ---------------------------------------------------------------------------
Write-Host ""
Write-Host "════════════════════════════════════════════════════════" -ForegroundColor Green
Write-Host "  Deployment complete  🚀"                               -ForegroundColor Green
Write-Host "════════════════════════════════════════════════════════" -ForegroundColor Green
Write-Host ("  {0,-28} {1}" -f "invoice-token:",       $InvoiceTokenId)
Write-Host ("  {0,-28} {1}" -f "invoice-escrow:",      $InvoiceEscrowId)
Write-Host ("  {0,-28} {1}" -f "payment-distributor:", $PaymentDistributorId)
Write-Host ""
Write-Host "  Network: $StellarNetwork"
Write-Host ""
Write-Warn "Copy the contract IDs above into README.md → Contract Addresses."

Pop-Location

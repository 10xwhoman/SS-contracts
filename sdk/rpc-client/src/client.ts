import {
  Account,
  Contract,
  Keypair,
  Networks,
  Operation,
  rpc,
  TransactionBuilder,
  xdr,
} from "@stellar/stellar-sdk";

export interface RpcClientOptions {
  contractId: string;
  rpcUrl: string;
  networkPassphrase?: string;
  timeoutMs?: number;
}

/** Small convenience wrapper for read and write calls to InvoiceEscrow. */
export class InvoiceEscrowRpcClient {
  readonly server: rpc.Server;
  readonly contractId: string;
  readonly networkPassphrase: string;

  constructor(options: RpcClientOptions) {
    this.contractId = options.contractId;
    this.networkPassphrase = options.networkPassphrase ?? Networks.TESTNET;
    this.server = new rpc.Server(options.rpcUrl, {
      allowHttp: options.rpcUrl.startsWith("http://"),
      timeout: options.timeoutMs,
    });
  }

  async query<T>(method: string, args: Record<string, unknown> = {}): Promise<T> {
    const { result } = await this.server.queryContract<T>(
      this.contractId,
      method,
      args,
      this.networkPassphrase,
    );
    return result;
  }

  getEscrow<T = unknown>(invoiceId: string): Promise<T> {
    return this.query<T>("get_escrow", { invoice_id: invoiceId });
  }

  getEscrowStatus<T = unknown>(invoiceId: string): Promise<T> {
    return this.query<T>("get_escrow_status", { invoice_id: invoiceId });
  }

  getConfig<T = unknown>(): Promise<T> {
    return this.query<T>("get_config");
  }

  /** Submit a contract method using its already encoded Soroban arguments. */
  async submit(method: string, args: xdr.ScVal[], signer: Keypair): Promise<rpc.Api.GetTransactionResponse> {
    const account: Account = await this.server.getAccount(signer.publicKey());
    const call = new Contract(this.contractId).call(method, ...args);
    const tx = new TransactionBuilder(account, {
      fee: "100",
      networkPassphrase: this.networkPassphrase,
    })
      .addOperation(call)
      .setTimeout(30)
      .build();
    const prepared = await this.server.prepareTransaction(tx);
    prepared.sign(signer);
    const sent = await this.server.sendTransaction(prepared);
    if (sent.status === "ERROR") {
      throw new Error(`RPC rejected transaction ${sent.hash}`);
    }
    return this.server.pollTransaction(sent.hash);
  }

  /** Exposes a prepared Operation for clients that need custom transaction composition. */
  contractCall(method: string, ...args: xdr.ScVal[]): Operation {
    return new Contract(this.contractId).call(method, ...args);
  }
}

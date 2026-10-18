import { Account, rpc, TransactionBuilder, xdr } from "@stellar/stellar-sdk";

export interface ResourceEstimate {
  minResourceFee: string;
  latestLedger: number;
  instructions: number;
  diskReadBytes: number;
  writeBytes: number;
  readOnlyLedgerKeys: number;
  readWriteLedgerKeys: number;
  footprint: xdr.LedgerFootprint;
  simulation: rpc.Api.SimulateTransactionSuccessResponse;
}

/** Simulate a transaction before signing and return its resource and fee estimate. */
export async function estimateTransactionResources(
  server: rpc.Server,
  transaction: ReturnType<TransactionBuilder["build"]>,
): Promise<ResourceEstimate> {
  const simulation = await server.simulateTransaction(transaction);
  if (rpc.Api.isSimulationError(simulation)) {
    throw new Error(`Soroban simulation failed: ${simulation.error}`);
  }
  const footprint = simulation.transactionData.getFootprint();
  const resources = simulation.transactionData.build().resources;
  return {
    minResourceFee: simulation.minResourceFee,
    latestLedger: simulation.latestLedger,
    instructions: resources.instructions,
    diskReadBytes: resources.diskReadBytes,
    writeBytes: resources.writeBytes,
    readOnlyLedgerKeys: footprint.readOnly.length,
    readWriteLedgerKeys: footprint.readWrite.length,
    footprint,
    simulation,
  };
}

/** Build an unsigned invocation and estimate its Soroban resource fee. */
export async function simulateContractCall(
  server: rpc.Server,
  source: Account,
  operation: xdr.Operation,
  networkPassphrase: string,
): Promise<ResourceEstimate> {
  const tx = new TransactionBuilder(source, {
    fee: "100",
    networkPassphrase,
  })
    .addOperation(operation)
    .setTimeout(30)
    .build();
  return estimateTransactionResources(server, tx);
}

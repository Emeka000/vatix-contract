/**
 * Turns a thrown contract-invocation error into a short, user-facing
 * revert reason for display in a toast/inline alert.
 */
export function parseContractError(err: unknown): string {
  const message = err instanceof Error ? err.message : String(err);

  // Soroban host trap, e.g. `Error(Contract, #10)` — surfaced as-is by the
  // SDK inside simulation/transaction failure messages.
  const contractError = message.match(/Error\(Contract,\s*#(\d+)\)/);
  if (contractError) {
    return `Contract rejected the transaction (error #${contractError[1]}).`;
  }

  const simulationFailed = message.match(/^Simulation failed:\s*([\s\S]+)$/);
  if (simulationFailed) {
    return `Transaction would fail: ${simulationFailed[1]}`;
  }

  const txFailed = message.match(/^Transaction failed:\s*([\s\S]+)$/);
  if (txFailed) {
    return `Transaction failed: ${txFailed[1]}`;
  }

  if (/denied|rejected|cancel/i.test(message)) {
    return "Request was rejected in the wallet.";
  }

  return message;
}

export function operationError(message: string, reason: unknown): string {
  const detail = reason instanceof Error ? reason.message : String(reason ?? "");
  const normalized = detail.trim();
  return normalized ? `${message} ${normalized}` : message;
}

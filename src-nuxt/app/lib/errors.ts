/** What went wrong, as the engine or the error itself says it. */
export function reasonOf(err: unknown): string {
  return err instanceof Error ? err.message : String(err)
}

/** What failed and why, the way every error is shown: never what failed alone. */
export function failure(what: string, err: unknown): string {
  const reason = reasonOf(err).trim()
  return reason ? `${what.replace(/[.:]$/, '')}: ${reason}` : what
}

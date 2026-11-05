export class WorkspaceFailure extends Error {
  constructor(
    readonly code: string,
    readonly requestId?: string,
    readonly runtimeId?: string,
  ) {
    super(code);
  }
}

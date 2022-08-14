import type * as urql from "@urql/core";

export class TauriInvokeIPCError extends Error {
  public override readonly name: string = "TauriInvokeIPCError";
  constructor(message: string, options?: ErrorOptions) {
    super(message, options);
  }
}

export class TauriInvokeAsyncGraphQLError extends AggregateError {
  public override readonly name: string = "TauriInvokeAsyncGraphQLError";
  constructor(errors: Iterable<unknown>, message?: string, options?: ErrorOptions) {
    super(errors, message, options);
  }
}

export const handleTauriInvokeError = (error: unknown & { toString(): string }) => {
  const message = "tauri command invocation failed due to IPC error";
  const cause = error instanceof Error ? error : new Error(error.toString());
  throw new TauriInvokeIPCError(message, { cause });
};

export const handleAsyncGraphQLError = (result: urql.ExecutionResult, isOk: boolean): void => {
  if (!isOk) {
    const errors = result.errors || [];
    const message = `tauri command invocation failed due to "async-graphql" error`;
    throw new TauriInvokeAsyncGraphQLError(errors, message);
  }
};

import * as graphql from "graphql";
import * as tauri from "@tauri-apps/api";
import * as urql from "@urql/core";
import type { ObserverLike, SubscriptionOperation } from "@urql/core/dist/types/exchanges/subscription";
import * as wonka from "wonka";

import * as error from "./error";

interface InvokeArgs {
  [key: string]: unknown;
}

type InvokeResponse = [body: string, isOk: boolean];

const intoGraphQLInvokeCommand = (operation: urql.Operation): string => {
  return `plugin:graphql-ipc|${operation.context.url}`;
};

const intoGraphQLInvokeArguments = (operation: urql.Operation): InvokeArgs => {
  return {
    query: graphql.print(operation.query),
    variables: operation.variables,
  };
};

const subscriptionInvokeCommand = "plugin:graphql-ipc|subscription";

const intoSubscriptionInvokeArguments = (operation: SubscriptionOperation, id: number): Record<string, unknown> => {
  return { ...operation, id };
};

const intoSubscriptionEvent = (id: number): string => {
  return `graphql://${id}`;
};

const intoExecutionResult = (response: InvokeResponse): urql.ExecutionResult => {
  const [body, isOk] = response;
  const result = JSON.parse(body) as urql.ExecutionResult;
  error.handleGraphQLError(result, isOk);
  return result;
};

const makeInvokeSource = (
  operation: urql.Operation,
  cmd: string,
  args?: InvokeArgs,
): wonka.Source<urql.OperationResult> => {
  return wonka.make<urql.OperationResult>((observer) => {
    tauri
      // invoke tauri command
      .invoke<InvokeResponse>(cmd, args)
      // catch errors from calling tauri IPC
      .catch(error.handleTauriInvokeError)
      // transform invoke result into urql execution result
      .then(intoExecutionResult)
      // transform execution result into urql operation result
      .then((executionResult) => {
        const operationResult = urql.makeResult(operation, executionResult);
        observer.next(operationResult);
      })
      // catch remaining errors (including those rethrown from above)
      .catch((error: Error) => {
        const errorResult = urql.makeErrorResult(operation, error, null);
        observer.next(errorResult);
      })
      // complete the source
      .finally(() => observer.complete());

    return () => {
      // noop
    };
  });
};

export const invokeExchange: urql.Exchange = (input) => {
  return (ops$) => {
    // kinds of operations to process
    const processKinds = new Set(["mutation", "query"]);

    // create a shared source so operation handling is not split
    const sharedOps$ = wonka.share(ops$);

    // create a teardown source for a given operation key
    const teardownForKey$ = (key: number) =>
      wonka.pipe(
        sharedOps$,
        wonka.filter((operation) => operation.kind === "teardown" && operation.key === key),
      );

    // create a source of processed operations
    const processedOps$ = wonka.pipe(
      sharedOps$,
      // process only "mutation" and "query" operations
      wonka.filter(({ kind }) => processKinds.has(kind)),
      // prepare to handle the operation as a tauri invocation
      wonka.mergeMap((operation) => {
        const cmd = intoGraphQLInvokeCommand(operation);
        const args = intoGraphQLInvokeArguments(operation);
        // create the source for the tauri invocation
        const invokeHandled$ = makeInvokeSource(operation, cmd, args);
        // create the source for tearing down the invocation in case it is aborted before completion
        const invokeAborted$ = teardownForKey$(operation.key);
        // race the invocation handler and invocation aborter
        return wonka.pipe(invokeHandled$, wonka.takeUntil(invokeAborted$));
      }),
    );

    // create a source of operations not processed
    const forwardedOps$ = wonka.pipe(
      sharedOps$,
      // forward "subsciption", "teardown", ..., except "mutation", "query"
      wonka.filter(({ kind }) => !processKinds.has(kind)),
      // pass the operations on to the next exchange
      input.forward,
    );

    // merge the processed and unprocessed ops back together
    return wonka.merge([processedOps$, forwardedOps$]);
  };
};

function subscribe(operation: SubscriptionOperation) {
  return (sink: ObserverLike<urql.ExecutionResult>) => {
    const id = Math.floor(Math.random() * 10000000);
    const event = intoSubscriptionEvent(id);

    tauri.event
      // set the event listener for subscription updates
      .listen<string | null>(event, (event) => {
        if (event.payload === null) {
          // when the payload is finally null, complete the sink
          sink.complete();
        } else {
          // otherwise, push the subscription update into the sink
          sink.next(JSON.parse(event.payload) as urql.ExecutionResult);
        }
      })
      // invoke the subscription query and start the stream
      .then(() => {
        const cmd = subscriptionInvokeCommand;
        const args = intoSubscriptionInvokeArguments(operation, id);
        return tauri.invoke(cmd, args);
      })
      // catch errors from calling tauri IPC
      .catch(error.handleTauriInvokeError);

    return {
      unsubscribe: () => {
        console.debug("unsubscribe called");
        sink.complete();
      },
    };
  };
}

export const forwardSubscription = (operation: SubscriptionOperation) => ({
  subscribe: subscribe(operation),
});

export { TauriInvokeGraphQLError, TauriInvokeIPCError } from "./error";

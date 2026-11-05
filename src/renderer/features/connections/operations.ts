import type { WorkspaceOperation, WorkspaceReply } from '@contracts';
import type { MessageKey } from '../../../shared/i18n/messages';
import { asyncFeedbackDelay } from '../../components/ui/use-delayed-busy';
import { WorkspaceFailure } from './workspace-failure';

export const operationDelay = asyncFeedbackDelay;
export interface OperationRequest {
  owner: string;
  action: MessageKey;
  label: string;
  execute: () => Promise<WorkspaceReply>;
  resources?: string[];
  onProgress?: (job: WorkspaceOperation) => void;
  onResult?: (result: WorkspaceReply) => void | Promise<void>;
}
export interface Operation extends OperationRequest {
  id: string;
  startedAt: number;
  runtimeId: string | null;
  phase: 'running' | 'cancelling' | 'failed' | 'unknown';
  visible: boolean;
  job?: WorkspaceOperation;
  error?: WorkspaceFailure;
  cancelError?: WorkspaceFailure;
  dismissed?: boolean;
  presentedLocally?: boolean;
}
export interface OperationTransport {
  runtimeId(): string | null;
  get(id: string): Promise<WorkspaceReply>;
  cancel(id: string): Promise<WorkspaceReply>;
  reconcile(requestId: string, runtimeId: string): Promise<WorkspaceReply>;
}
export const operationBusy = (op: Operation) => op.phase !== 'failed';
export const operationRunning = (op: Operation) =>
  op.phase === 'running' || op.phase === 'cancelling';
export const operationHasVisibleFeedback = (op: Operation, anchors: ReadonlySet<string>) =>
  anchors.has(op.owner) || !!(op.job?.sessionId && anchors.has('session:' + op.job.sessionId));

// 与 Rust begin(..., cancellable) 保持一致；保存、删除和凭据结算不能假报已取消。
export const operationCanCancel = (op: Operation) =>
  !!op.job &&
  ['working', 'trust'].includes(op.job.phase) &&
  ['connect', 'test', 'credential'].includes(op.job.kind);
export const operationCanReconcile = (op: Operation) =>
  !!op.job || !!(op.error?.requestId && op.error?.runtimeId);

/** 记录与可见实例解耦；锁在执行延迟函数前同步获取，未知结果只核对原句柄。 */
export class OperationStore {
  private snapshot: { operations: Operation[]; anchors: ReadonlySet<string> } = {
    operations: [],
    anchors: new Set(),
  };
  private listeners = new Set<() => void>();
  private anchors = new Map<string, string>();
  private timers = new Map<string, ReturnType<typeof setTimeout>>();
  private reconciling = new Set<string>();
  private disposed = false;
  constructor(private transport: OperationTransport) {}
  subscribe = (listener: () => void) => {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  };
  getSnapshot = () => this.snapshot;
  private publish(operations = this.snapshot.operations) {
    if (this.disposed) return;
    const anchors = new Set(this.anchors.values());
    this.snapshot = {
      anchors,
      operations: operations.map((op) =>
        op.phase === 'failed' && !op.presentedLocally && operationHasVisibleFeedback(op, anchors)
          ? { ...op, presentedLocally: true }
          : op,
      ),
    };
    this.listeners.forEach((l) => l());
  }
  private update(id: string, change: Partial<Operation>) {
    this.publish(this.snapshot.operations.map((o) => (o.id === id ? { ...o, ...change } : o)));
  }
  setAnchor = (id: string, owner: string, visible: boolean) => {
    if (visible ? this.anchors.get(id) === owner : !this.anchors.has(id)) return;
    if (visible) this.anchors.set(id, owner);
    else this.anchors.delete(id);
    this.publish();
  };
  isBusy = (...resources: string[]) =>
    this.snapshot.operations.some(
      (op) =>
        operationBusy(op) && [op.owner, ...(op.resources ?? [])].some((r) => resources.includes(r)),
    );
  forOwner = (owner: string) =>
    this.snapshot.operations.find((o) => o.owner === owner && operationBusy(o)) ??
    [...this.snapshot.operations].reverse().find((o) => o.owner === owner);
  dismiss = (id: string) => {
    const op = this.snapshot.operations.find((o) => o.id === id);
    if (!op) return;
    if (operationBusy(op)) this.update(id, { dismissed: true });
    else this.publish(this.snapshot.operations.filter((o) => o.id !== id));
  };
  dismissOwnerFailures(owner: string) {
    this.publish(
      this.snapshot.operations.filter((op) => op.owner !== owner || op.phase !== 'failed'),
    );
  }
  report = (request: Omit<OperationRequest, 'execute'>, error: WorkspaceFailure) => {
    if (error.code === 'cancelled' || error.code === 'native_prompt_cancelled') return;
    if (this.snapshot.operations.some((o) => o.owner === request.owner && o.error === error))
      return;
    this.publish([
      ...this.snapshot.operations,
      {
        ...request,
        execute: () => Promise.reject(error),
        id: crypto.randomUUID(),
        startedAt: Date.now(),
        runtimeId: this.transport.runtimeId(),
        phase: error.code === 'result_unknown' ? 'unknown' : 'failed',
        visible: true,
        error,
      },
    ]);
  };
  run = async (request: OperationRequest): Promise<WorkspaceReply> => {
    if (this.disposed) throw new WorkspaceFailure('runtime_unavailable');
    if (this.isBusy(request.owner, ...(request.resources ?? [])))
      throw new WorkspaceFailure('busy');
    const op: Operation = {
      ...request,
      id: crypto.randomUUID(),
      startedAt: Date.now(),
      runtimeId: this.transport.runtimeId(),
      phase: 'running',
      visible: false,
    };
    this.publish([...this.snapshot.operations, op]);
    this.timers.set(
      op.id,
      setTimeout(() => {
        this.timers.delete(op.id);
        this.update(op.id, { visible: true });
      }, operationDelay),
    );
    return this.wait(op, request.execute);
  };
  private async settle(op: Operation, reply: WorkspaceReply): Promise<WorkspaceReply> {
    if (reply.kind !== 'operation') return reply;
    let job = reply.operation;
    for (;;) {
      this.update(op.id, { job });
      op.onProgress?.(job);
      if (!['working', 'trust'].includes(job.phase)) {
        if (job.phase !== 'success') throw new WorkspaceFailure(job.error ?? 'cancelled');
        return { kind: 'operation', operation: job };
      }
      if (this.disposed || this.transport.runtimeId() !== op.runtimeId)
        throw new WorkspaceFailure('result_unknown');
      await new Promise<void>((r) => setTimeout(r, 120));
      try {
        const next = await this.transport.get(job.id);
        if (
          next.kind !== 'operation' ||
          next.operation.id !== job.id ||
          this.transport.runtimeId() !== op.runtimeId
        )
          throw new WorkspaceFailure('result_unknown');
        job = next.operation;
      } catch {
        throw new WorkspaceFailure('result_unknown');
      }
    }
  }
  private async wait(
    op: Operation,
    execute: () => Promise<WorkspaceReply>,
  ): Promise<WorkspaceReply> {
    try {
      const reply = await this.settle(op, await execute());
      await op.onResult?.(reply);
      this.publish(
        this.snapshot.operations.filter(
          (o) =>
            o.id !== op.id &&
            !(o.owner === op.owner && o.action === op.action && !operationRunning(o)),
        ),
      );
      return reply;
    } catch (error) {
      const fault =
        error instanceof WorkspaceFailure ? error : new WorkspaceFailure('runtime_unavailable');
      if (['cancelled', 'native_prompt_cancelled'].includes(fault.code))
        this.publish(this.snapshot.operations.filter((o) => o.id !== op.id));
      else {
        this.publish(
          this.snapshot.operations.filter(
            (current) =>
              current.id === op.id ||
              current.owner !== op.owner ||
              current.action !== op.action ||
              operationRunning(current),
          ),
        );
        this.update(op.id, {
          phase: fault.code === 'result_unknown' ? 'unknown' : 'failed',
          visible: true,
          error: fault,
        });
      }
      throw fault;
    } finally {
      clearTimeout(this.timers.get(op.id));
      this.timers.delete(op.id);
    }
  }
  cancel = async (id: string) => {
    const op = this.snapshot.operations.find((o) => o.id === id);
    if (!op || !operationCanCancel(op) || op.phase !== 'running') return;
    if (op.runtimeId !== this.transport.runtimeId()) {
      this.update(id, {
        phase: 'unknown',
        visible: true,
        error: new WorkspaceFailure('result_unknown'),
      });
      return;
    }
    this.update(id, { phase: 'cancelling', cancelError: undefined });
    try {
      await this.transport.cancel(op.job!.id);
    } catch (error) {
      this.update(id, {
        phase: 'running',
        cancelError:
          error instanceof WorkspaceFailure ? error : new WorkspaceFailure('runtime_unavailable'),
      });
    }
  };
  reconcile = async (id: string) => {
    const op = this.snapshot.operations.find((o) => o.id === id);
    if (!op || op.phase !== 'unknown' || !operationCanReconcile(op) || this.reconciling.has(id))
      return;
    this.reconciling.add(id);
    this.update(id, { phase: 'running', error: undefined, dismissed: false });
    try {
      return await this.wait(op, async () => {
        const runtimeId = op.error?.runtimeId ?? op.runtimeId;
        if (runtimeId !== this.transport.runtimeId())
          throw new WorkspaceFailure('result_unknown', op.error?.requestId, runtimeId ?? undefined);
        let r: WorkspaceReply;
        try {
          if (op.job) {
            const jobReply = await this.transport.get(op.job.id);
            if (jobReply.kind !== 'operation' || jobReply.operation.id !== op.job.id)
              throw new Error('invalid operation');
            return jobReply;
          }
          r = await this.transport.reconcile(op.error!.requestId!, runtimeId!);
        } catch {
          throw new WorkspaceFailure('result_unknown', op.error?.requestId, runtimeId ?? undefined);
        }
        if (r.kind !== 'reconciled' || !r.result)
          throw new WorkspaceFailure(
            r.kind === 'reconciled' ? (r.error ?? 'result_unknown') : 'result_unknown',
            op.error?.requestId,
            runtimeId ?? undefined,
          );
        return r.result;
      });
    } finally {
      this.reconciling.delete(id);
    }
  };
  dispose() {
    this.disposed = true;
    this.timers.forEach(clearTimeout);
    this.timers.clear();
    this.listeners.clear();
    this.anchors.clear();
  }
}

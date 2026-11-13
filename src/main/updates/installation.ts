export interface InstallationPorts {
  confirm(): Promise<boolean>;
  lock(value: boolean): void;
  flush(): Promise<void>;
  shutdown(): Promise<void>;
  recover(): void;
}
/** Native preparation cannot be cancelled; a timeout can still be followed by a staged update. */
export class NativeUpdateUncertain extends Error {}
/** Saves must succeed before Squirrel is armed. After preparation, exit commits the approved install. */
export class UpdateInstallation {
  private running?: Promise<boolean>;
  constructor(private ports: InstallationPorts) {}
  install(prepare: () => Promise<void>, commit: () => void): Promise<boolean> {
    if (this.running) return this.running;
    this.running = (async () => {
      if (!(await this.ports.confirm())) return false;
      this.ports.lock(true);
      let armed = false;
      try {
        await this.ports.flush();
        await prepare();
        armed = true;
        await this.ports.shutdown();
        commit();
        return true;
      } catch (error) {
        if (armed || error instanceof NativeUpdateUncertain) {
          const needsShutdown = !armed;
          armed = true;
          if (needsShutdown) await this.ports.shutdown().catch(() => undefined);
          this.ports.recover();
        }
        throw error;
      } finally {
        if (!armed) this.ports.lock(false);
      }
    })().finally(() => {
      this.running = undefined;
    });
    return this.running;
  }
}

// Runs cron jobs independently: each gets its own waitUntil and its own
// catch, so one job throwing can neither reject the others nor settle the
// invocation early (a rejected Promise.all lets the runtime wind the isolate
// down while sibling jobs are still mid-flight).

export type ScheduledJob = {
  name: string;
  run: () => Promise<unknown>;
};

export type ScheduledJobResult = {
  name: string;
  ok: boolean;
  durationMs: number;
  error?: string;
};

function describe(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}

export function runScheduledJobs(
  jobs: ScheduledJob[],
  waitUntil: (promise: Promise<unknown>) => void,
): Promise<ScheduledJobResult[]> {
  const tracked = jobs.map((job) => {
    const promise = (async (): Promise<ScheduledJobResult> => {
      const startedAt = Date.now();
      try {
        await job.run();
        return { name: job.name, ok: true, durationMs: Date.now() - startedAt };
      } catch (error) {
        const message = describe(error);
        // Log the message explicitly: an Error object alone logs only its
        // stack frames in Workers observability.
        console.error(`${job.name} failed: ${message}`);
        return {
          name: job.name,
          ok: false,
          durationMs: Date.now() - startedAt,
          error: message,
        };
      }
    })();
    waitUntil(promise);
    return promise;
  });
  return Promise.all(tracked);
}

/**
 * Picks the jobs for a cron expression. Unknown expressions (e.g. a manual
 * "Trigger" from the dashboard with a stale schedule) run everything.
 */
export function selectScheduledJobs<K extends string>(
  cron: string,
  groups: Record<K, { crons: string[]; jobs: ScheduledJob[] }>,
): ScheduledJob[] {
  const all = Object.values(groups) as Array<{
    crons: string[];
    jobs: ScheduledJob[];
  }>;
  const matched = all.filter((group) => group.crons.includes(cron));
  return (matched.length ? matched : all).flatMap((group) => group.jobs);
}

/**
 * Whether a cron run is the day's slot for daily work: the run scheduled at
 * 00:00 UTC, or a run whose expression is not one of `knownCrons` (a manual
 * "Trigger" from the dashboard, which runs everything).
 */
export function isDailySlot(
  cron: string,
  scheduledTime: number,
  knownCrons: readonly string[],
): boolean {
  if (!knownCrons.includes(cron)) return true;
  const at = new Date(scheduledTime);
  return at.getUTCHours() === 0 && at.getUTCMinutes() === 0;
}

/**
 * Keeps each job's last run in D1 `scheduled_job_runs` (migration 041), for
 * the admin console. Best effort: a failed write is logged, never thrown.
 */
export async function recordScheduledJobRuns(
  db: Pick<D1Database, "prepare" | "batch">,
  cron: string,
  results: readonly ScheduledJobResult[],
  now: Date = new Date(),
): Promise<void> {
  if (results.length === 0) return;
  const ranAt = now.toISOString();
  try {
    await db.batch(
      results.map((result) =>
        db
          .prepare(
            `INSERT INTO scheduled_job_runs (name, cron, ok, duration_ms, error, ran_at)
             VALUES (?, ?, ?, ?, ?, ?)
             ON CONFLICT(name) DO UPDATE SET
               cron = excluded.cron,
               ok = excluded.ok,
               duration_ms = excluded.duration_ms,
               error = excluded.error,
               ran_at = excluded.ran_at`,
          )
          .bind(
            result.name,
            cron,
            result.ok ? 1 : 0,
            Math.max(0, Math.round(result.durationMs)),
            result.error ? result.error.slice(0, 500) : null,
            ranAt,
          ),
      ),
    );
  } catch (error) {
    console.warn(`scheduled_job_runs write failed: ${describe(error)}`);
  }
}

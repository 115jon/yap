import packageJson from "../../package.json";

type Clock = () => Date;

export function buildHealthzPayload(now: Clock = () => new Date()) {
  // Stamped by vite.config.ts at build; the admin console's "deployed"
  // readout reads them through its service binding.
  const commit = import.meta.env.YAP_BUILD_COMMIT;
  const builtAt = import.meta.env.YAP_BUILT_AT;
  return {
    ok: true,
    service: "ralph-meet",
    version: packageJson.version,
    ...(commit ? { commit } : {}),
    ...(builtAt ? { builtAt } : {}),
    checkedAt: now().toISOString(),
  };
}

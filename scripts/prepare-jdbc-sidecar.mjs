import { copyFileSync, mkdirSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = dirname(dirname(fileURLToPath(import.meta.url)));
const maven = process.platform === "win32" ? "mvn.cmd" : "mvn";
const result = spawnSync(maven, ["-q", "-f", join(repoRoot, "jdbc-sidecar", "pom.xml"), "package"], {
  cwd: repoRoot,
  stdio: "inherit",
  shell: process.platform === "win32"
});

if (result.error) throw result.error;
if (result.status !== 0) process.exit(result.status ?? 1);

const resources = join(repoRoot, "resources");
mkdirSync(resources, { recursive: true });
copyFileSync(
  join(repoRoot, "jdbc-sidecar", "target", "datanexa-jdbc-sidecar.jar"),
  join(resources, "datanexa-jdbc-sidecar.jar")
);
console.log(`Prepared JDBC sidecar at ${join(resources, "datanexa-jdbc-sidecar.jar")}`);

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const rootDir = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const configPath = path.join(rootDir, "app.config.json");
const packagePath = path.join(rootDir, "package.json");
const tauriConfigPath = path.join(rootDir, "src-tauri", "tauri.conf.json");
const cargoTomlPath = path.join(rootDir, "src-tauri", "Cargo.toml");
const cargoLockPath = path.join(rootDir, "src-tauri", "Cargo.lock");

const semverPattern =
  /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?(?:\+[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?$/;

function readJson(filePath) {
  return JSON.parse(fs.readFileSync(filePath, "utf8"));
}

function writeJson(filePath, value) {
  fs.writeFileSync(filePath, `${JSON.stringify(value, null, 2)}\n`);
}

function replaceTomlPackageVersion(content, version, filePath) {
  const lines = content.split(/\r?\n/);
  const eol = content.includes("\r\n") ? "\r\n" : "\n";
  let inPackageSection = false;
  let replaced = false;

  const nextLines = lines.map((line) => {
    const sectionMatch = line.match(/^\s*\[([^\]]+)\]\s*$/);

    if (sectionMatch) {
      inPackageSection = sectionMatch[1] === "package";
      return line;
    }

    if (inPackageSection && /^\s*version\s*=/.test(line)) {
      replaced = true;
      return line.replace(/^(\s*version\s*=\s*)"[^"]*"(.*)$/, `$1"${version}"$2`);
    }

    return line;
  });

  if (!replaced) {
    throw new Error(`Unable to find [package].version in ${filePath}`);
  }

  return nextLines.join(eol);
}

function replaceCargoLockPackageVersion(content, packageName, version, filePath) {
  const escapedPackageName = packageName.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const packagePattern = new RegExp(
    `(^\\[\\[package\\]\\]\\r?\\nname = "${escapedPackageName}"\\r?\\nversion = )"[^"]+"`,
    "m"
  );

  if (!packagePattern.test(content)) {
    throw new Error(`Unable to find ${packageName} package version in ${filePath}`);
  }

  return content.replace(packagePattern, `$1"${version}"`);
}

const appConfig = readJson(configPath);
const version = appConfig.version;

if (typeof version !== "string" || !semverPattern.test(version)) {
  throw new Error("app.config.json version must be a valid semantic version, such as 0.1.0");
}

const packageJson = readJson(packagePath);
packageJson.version = version;
writeJson(packagePath, packageJson);

const tauriConfig = readJson(tauriConfigPath);
tauriConfig.version = version;
writeJson(tauriConfigPath, tauriConfig);

const cargoToml = fs.readFileSync(cargoTomlPath, "utf8");
fs.writeFileSync(cargoTomlPath, replaceTomlPackageVersion(cargoToml, version, cargoTomlPath));

if (fs.existsSync(cargoLockPath)) {
  const cargoLock = fs.readFileSync(cargoLockPath, "utf8");
  fs.writeFileSync(cargoLockPath, replaceCargoLockPackageVersion(cargoLock, "datanexa", version, cargoLockPath));
}

console.log(`Synced app version ${version}.`);

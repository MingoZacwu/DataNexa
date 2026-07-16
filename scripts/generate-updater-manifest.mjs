#!/usr/bin/env node

import { readFileSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";

const [releaseJsonArg, signatureDirArg, outputArg] = process.argv.slice(2);

if (!releaseJsonArg || !signatureDirArg || !outputArg) {
  console.error(
    "Usage: generate-updater-manifest.mjs <release.json> <signature-dir> <output.json>"
  );
  process.exit(1);
}

const releaseJsonPath = resolve(releaseJsonArg);
const signatureDir = resolve(signatureDirArg);
const outputPath = resolve(outputArg);
const release = JSON.parse(readFileSync(releaseJsonPath, "utf8"));
const assets = Array.isArray(release.assets) ? release.assets : [];
const releaseTag = String(release.tag_name || "");
const version = releaseTag.replace(/^v/, "");

if (!releaseTag || !version) {
  throw new Error("Release tag_name is missing or invalid");
}

function findSingleAsset(suffix) {
  const matches = assets.filter((asset) => asset.name.endsWith(suffix));

  if (matches.length !== 1) {
    throw new Error(
      `Expected exactly one release asset ending with ${suffix}, found ${matches.length}`
    );
  }

  return matches[0];
}

function createPlatformEntry(signatureSuffix) {
  const signatureAsset = findSingleAsset(signatureSuffix);
  const bundleName = signatureAsset.name.slice(0, -".sig".length);
  const bundleAsset = assets.find((asset) => asset.name === bundleName);

  if (!bundleAsset) {
    throw new Error(`Updater bundle ${bundleName} was not found in release assets`);
  }

  const signaturePath = join(signatureDir, signatureAsset.name);
  const signature = readFileSync(signaturePath, "utf8").trim();

  if (!signature) {
    throw new Error(`Updater signature ${signatureAsset.name} is empty`);
  }

  return {
    signature,
    url: stableAssetDownloadUrl(bundleAsset),
  };
}

function stableAssetDownloadUrl(asset) {
  const url = new URL(asset.browser_download_url);
  const marker = "/releases/download/";
  const markerIndex = url.pathname.indexOf(marker);

  if (url.protocol !== "https:" || markerIndex <= 0) {
    throw new Error(`Release asset ${asset.name} has an invalid browser_download_url`);
  }

  const repositoryPath = url.pathname.slice(0, markerIndex);
  url.pathname = `${repositoryPath}${marker}${encodeURIComponent(releaseTag)}/${encodeURIComponent(asset.name)}`;
  url.search = "";
  url.hash = "";
  return url.toString();
}

const windows = createPlatformEntry(".exe.sig");
const macosUniversal = createPlatformEntry(".app.tar.gz.sig");

const manifest = {
  version,
  pub_date: new Date().toISOString(),
  platforms: {
    "windows-x86_64": windows,
    "windows-x86_64-nsis": windows,
    "darwin-universal": macosUniversal,
  },
};

writeFileSync(outputPath, `${JSON.stringify(manifest, null, 2)}\n`);
console.log(`Generated updater manifest ${outputPath} for ${version}`);

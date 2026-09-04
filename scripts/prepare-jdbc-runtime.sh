#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "$0")/.." && pwd)"
jdk_home="${1:-${JAVA_HOME:-}}"
if [[ -z "$jdk_home" ]]; then
  echo "Pass a JDK 21 home or set JAVA_HOME." >&2
  exit 1
fi
if [[ ! -x "$jdk_home/bin/jlink" ]]; then
  echo "jlink was not found under the selected JDK." >&2
  exit 1
fi
java_version="$($jdk_home/bin/java -version 2>&1 | head -n 1)"
if [[ ! "$java_version" =~ version\ \"21\. ]]; then
  echo "DataNexa JDBC release runtime requires JDK 21. Detected: $java_version" >&2
  exit 1
fi

mvn -q -f "$repo_root/jdbc-sidecar/pom.xml" package
runtime_root="$repo_root/resources/jdbc-runtime"
mkdir -p "$runtime_root"
build_parent="$(mktemp -d "${TMPDIR:-/tmp}/datanexa-jdbc-runtime.XXXXXX")"
build_runtime="$build_parent/runtime"
trap 'rm -rf "$build_parent"' EXIT
"$jdk_home/bin/jlink" \
  --add-modules java.base,java.sql,java.naming,java.logging,java.xml,java.management,java.desktop,java.net.http,java.security.jgss,jdk.crypto.ec,jdk.unsupported \
  --strip-debug \
  --no-header-files \
  --no-man-pages \
  --compress=2 \
  --output "$build_runtime"
rm -rf "$runtime_root/bin" "$runtime_root/conf" "$runtime_root/include" "$runtime_root/legal" "$runtime_root/lib" "$runtime_root/release"
# macOS assigns com.apple.provenance to symlinks and does not allow that
# attribute to be removed later by Cargo or Tauri. Dereference the JDK's
# legal-file links and avoid copying filesystem metadata into the bundle.
cp -RLX "$build_runtime"/. "$runtime_root"/
cp "$repo_root/jdbc-sidecar/target/datanexa-jdbc-sidecar.jar" "$runtime_root/lib/datanexa-jdbc-sidecar.jar"
# The JDK legal tree contains macOS provenance metadata that Cargo cannot
# inspect reliably during its recursive resource scan. Preserve the notices in
# a compressed archive, then keep the problematic tree out of the bundle.
tar -czf "$runtime_root/jdk-legal-notices.tar.gz" -C "$runtime_root" legal
rm -rf "$runtime_root/legal"
# Tauri removes extended attributes recursively while bundling on macOS, so
# ensure the generated runtime is writable by its owner.
find -L "$runtime_root" -type f -exec chmod u+w {} +
echo "Prepared DataNexa JDBC runtime at $runtime_root"

import { spawnSync } from 'node:child_process';

const platformCommands = {
  win32: {
    command: 'powershell',
    args: ['-ExecutionPolicy', 'Bypass', '-File', 'scripts/prepare-jdbc-runtime.ps1'],
  },
  darwin: {
    command: 'bash',
    args: ['scripts/prepare-jdbc-runtime.sh'],
  },
};

const selected = platformCommands[process.platform];
if (!selected) {
  console.error(`Bundled JDBC runtime is supported only on Windows and macOS (current platform: ${process.platform}).`);
  process.exit(1);
}

const result = spawnSync(selected.command, selected.args, {
  cwd: process.cwd(),
  stdio: 'inherit',
  shell: process.platform === 'win32',
});

if (result.error) {
  console.error(`Failed to start JDBC runtime preparation: ${result.error.message}`);
  process.exit(1);
}

process.exit(result.status ?? 1);

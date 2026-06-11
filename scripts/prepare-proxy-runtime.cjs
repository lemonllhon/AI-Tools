const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const https = require('node:https');
const http = require('node:http');
const zlib = require('node:zlib');
const { spawnSync } = require('node:child_process');

const repoRoot = path.resolve(__dirname, '..');
const runtimeRoot = path.join(repoRoot, 'src-tauri', 'proxy-runtime');
const bundleRoot = path.join(repoRoot, 'src-tauri', 'proxy-runtime-bundle');
const runtimeManifestPath = path.join(runtimeRoot, 'runtime-manifest.json');
const runtimeSourcesPath = path.join(runtimeRoot, 'runtime-sources.json');
const downloadsDir = path.join(runtimeRoot, 'downloads');
const tempDir = path.join(runtimeRoot, '.tmp');

const supportedTargets = new Set([
  'windows-x86_64',
  'darwin-x86_64',
  'darwin-aarch64',
  'linux-x86_64',
  'linux-aarch64',
]);
const runtimes = new Set(['xray', 'sing-box', 'mihomo']);

function usage() {
  return [
    'Usage: node scripts/prepare-proxy-runtime.cjs [--targets target[,target...]] [--all] [--offline]',
    '',
    'Environment:',
    '  PROXY_RUNTIME_TARGETS   Comma-separated target list. Overrides host target detection.',
    '  COCKPIT_SKIP_PROXY_RUNTIME=1   Skip preparation for local emergency builds.',
  ].join('\n');
}

function parseArgs(argv) {
  const options = {
    all: false,
    offline: false,
    targets: null,
  };

  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    if (arg === '--help' || arg === '-h') {
      console.log(usage());
      process.exit(0);
    }
    if (arg === '--all') {
      options.all = true;
      continue;
    }
    if (arg === '--offline') {
      options.offline = true;
      continue;
    }
    if (arg === '--targets') {
      index += 1;
      if (!argv[index]) {
        throw new Error('--targets requires a comma-separated value');
      }
      options.targets = splitTargets(argv[index]);
      continue;
    }
    if (arg.startsWith('--targets=')) {
      options.targets = splitTargets(arg.slice('--targets='.length));
      continue;
    }
    throw new Error(`Unknown argument: ${arg}`);
  }

  return options;
}

function splitTargets(value) {
  return value
    .split(',')
    .map((target) => target.trim())
    .filter(Boolean);
}

function detectHostTarget() {
  const arch = process.arch;
  if (process.platform === 'win32' && arch === 'x64') {
    return 'windows-x86_64';
  }
  if (process.platform === 'darwin' && arch === 'x64') {
    return 'darwin-x86_64';
  }
  if (process.platform === 'darwin' && arch === 'arm64') {
    return 'darwin-aarch64';
  }
  if (process.platform === 'linux' && arch === 'x64') {
    return 'linux-x86_64';
  }
  if (process.platform === 'linux' && arch === 'arm64') {
    return 'linux-aarch64';
  }
  throw new Error(`Unsupported host platform for proxy runtime: ${process.platform}/${arch}`);
}

function resolveTargets(options) {
  let targets = options.targets;
  if (!targets && process.env.PROXY_RUNTIME_TARGETS) {
    targets = splitTargets(process.env.PROXY_RUNTIME_TARGETS);
  }
  if (options.all) {
    targets = Array.from(supportedTargets);
  }
  if (!targets || targets.length === 0) {
    targets = [detectHostTarget()];
  }

  const uniqueTargets = Array.from(new Set(targets));
  for (const target of uniqueTargets) {
    if (!supportedTargets.has(target)) {
      throw new Error(`Unsupported proxy runtime target: ${target}`);
    }
  }
  return uniqueTargets;
}

function readJson(filePath) {
  try {
    return JSON.parse(fs.readFileSync(filePath, 'utf8'));
  } catch (error) {
    throw new Error(`Failed to read JSON ${filePath}: ${error.message}`);
  }
}

function assertSha256(value, label) {
  if (typeof value !== 'string' || !/^[a-f0-9]{64}$/i.test(value)) {
    throw new Error(`${label} must be a 64-character sha256 hex string`);
  }
}

function isAutoSha256(value) {
  return value === 'auto';
}

function validateManifest(manifest, sources) {
  if (manifest.schemaVersion !== 1) {
    throw new Error('runtime-manifest.json schemaVersion must be 1');
  }
  if (!Array.isArray(manifest.files)) {
    throw new Error('runtime-manifest.json files must be an array');
  }
  if (sources.schemaVersion !== 1) {
    throw new Error('runtime-sources.json schemaVersion must be 1');
  }
  if (!Array.isArray(sources.sources)) {
    throw new Error('runtime-sources.json sources must be an array');
  }

  const sourceByKey = new Map();
  for (const source of sources.sources) {
    validateSource(source);
    const key = entryKey(source);
    if (sourceByKey.has(key)) {
      throw new Error(`Duplicate runtime source for ${key}`);
    }
    sourceByKey.set(key, source);
  }

  const manifestByKey = new Map();
  for (const entry of manifest.files) {
    validateManifestEntry(entry);
    const key = entryKey(entry);
    if (manifestByKey.has(key)) {
      throw new Error(`Duplicate runtime manifest entry for ${key}`);
    }
    const source = sourceByKey.get(key);
    if (!source) {
      throw new Error(`Missing runtime source for ${key}`);
    }
    if (source.version !== entry.version) {
      throw new Error(`Version mismatch for ${key}: manifest ${entry.version}, source ${source.version}`);
    }
    if (source.destPath !== entry.path) {
      throw new Error(`Path mismatch for ${key}: manifest ${entry.path}, source ${source.destPath}`);
    }
    manifestByKey.set(key, entry);
  }

  for (const target of supportedTargets) {
    for (const runtime of runtimes) {
      const key = `${target}:${runtime}`;
      if (!manifestByKey.has(key)) {
        throw new Error(`runtime-manifest.json missing ${key}`);
      }
      if (!sourceByKey.has(key)) {
        throw new Error(`runtime-sources.json missing ${key}`);
      }
    }
  }
}

function validateSource(source) {
  validateCommonEntry(source, 'runtime-sources.json');
  assertSha256(source.archiveSha256, `${entryKey(source)} archiveSha256`);
  if (!['zip', 'tar.gz', 'gz'].includes(source.archiveType)) {
    throw new Error(`${entryKey(source)} archiveType must be zip, tar.gz, or gz`);
  }
  for (const field of ['id', 'url', 'archiveBinaryPath', 'destPath']) {
    if (typeof source[field] !== 'string' || !source[field]) {
      throw new Error(`${entryKey(source)} source field ${field} must be a non-empty string`);
    }
  }
  if (!source.destPath.startsWith(`bin/${source.target}/`)) {
    throw new Error(`${entryKey(source)} destPath must stay under bin/${source.target}/`);
  }
}

function validateManifestEntry(entry) {
  validateCommonEntry(entry, 'runtime-manifest.json');
  if (!isAutoSha256(entry.sha256)) {
    assertSha256(entry.sha256, `${entryKey(entry)} sha256`);
  }
  if (typeof entry.path !== 'string' || !entry.path.startsWith(`bin/${entry.target}/`)) {
    throw new Error(`${entryKey(entry)} path must stay under bin/${entry.target}/`);
  }
}

function validateCommonEntry(entry, label) {
  if (!supportedTargets.has(entry.target)) {
    throw new Error(`${label} has unsupported target: ${entry.target}`);
  }
  if (!runtimes.has(entry.runtime)) {
    throw new Error(`${label} has unsupported runtime: ${entry.runtime}`);
  }
  if (typeof entry.version !== 'string' || !entry.version) {
    throw new Error(`${entryKey(entry)} version must be a non-empty string`);
  }
}

function entryKey(entry) {
  return `${entry.target}:${entry.runtime}`;
}

function sha256File(filePath) {
  const hash = crypto.createHash('sha256');
  const file = fs.readFileSync(filePath);
  hash.update(file);
  return hash.digest('hex');
}

function assertInside(parentDir, childPath, label) {
  const relative = path.relative(parentDir, childPath);
  if (relative === '' || (!relative.startsWith('..') && !path.isAbsolute(relative))) {
    return;
  }
  throw new Error(`${label} escapes expected directory: ${childPath}`);
}

function runtimePath(relativePath) {
  const resolved = path.join(runtimeRoot, relativePath);
  assertInside(runtimeRoot, resolved, relativePath);
  return resolved;
}

async function ensureRuntimeFile(entry, source, options) {
  const destination = runtimePath(entry.path);
  const expectedSha = isAutoSha256(entry.sha256) ? null : entry.sha256;
  if (fs.existsSync(destination)) {
    const actualSha = sha256File(destination);
    if (expectedSha && actualSha !== expectedSha) {
      throw new Error(
        `${entryKey(entry)} sha256 mismatch at ${destination}. Expected ${expectedSha}, got ${actualSha}. Delete the file and rerun preparation to restore it.`
      );
    }
    if (expectedSha || options.offline) {
      return { destination, sha256: actualSha };
    }
  }

  if (options.offline) {
    throw new Error(`${entryKey(entry)} missing at ${destination} and --offline was set`);
  }

  fs.mkdirSync(downloadsDir, { recursive: true });
  fs.mkdirSync(tempDir, { recursive: true });

  const archivePath = path.join(downloadsDir, archiveFileName(source));
  await ensureArchive(source, archivePath);

  const extractDir = path.join(tempDir, source.id);
  fs.rmSync(extractDir, { recursive: true, force: true });
  fs.mkdirSync(extractDir, { recursive: true });
  extractArchive(archivePath, extractDir, source.archiveType, source.archiveBinaryPath);

  const extractedBinary = path.join(extractDir, ...source.archiveBinaryPath.split('/'));
  assertInside(extractDir, extractedBinary, source.archiveBinaryPath);
  if (!fs.existsSync(extractedBinary)) {
    throw new Error(`${entryKey(entry)} archive did not contain ${source.archiveBinaryPath}`);
  }

  fs.mkdirSync(path.dirname(destination), { recursive: true });
  fs.copyFileSync(extractedBinary, destination);
  chmodExecutable(destination);

  const actualSha = sha256File(destination);
  if (expectedSha && actualSha !== expectedSha) {
    throw new Error(`${entryKey(entry)} extracted sha256 mismatch. Expected ${expectedSha}, got ${actualSha}`);
  }
  return { destination, sha256: actualSha };
}

function archiveFileName(source) {
  const urlPath = new URL(source.url).pathname;
  const baseName = path.posix.basename(urlPath);
  return `${source.id}-${baseName}`;
}

async function ensureArchive(source, archivePath) {
  if (fs.existsSync(archivePath)) {
    const actualSha = sha256File(archivePath);
    if (actualSha !== source.archiveSha256) {
      throw new Error(
        `${entryKey(source)} archive sha256 mismatch at ${archivePath}. Expected ${source.archiveSha256}, got ${actualSha}. Delete the cached archive and rerun preparation.`
      );
    }
    return;
  }

  const tempArchivePath = `${archivePath}.part`;
  fs.rmSync(tempArchivePath, { force: true });
  console.log(`[proxy-runtime] downloading ${source.id}`);
  await download(source.url, tempArchivePath);

  const actualSha = sha256File(tempArchivePath);
  if (actualSha !== source.archiveSha256) {
    fs.rmSync(tempArchivePath, { force: true });
    throw new Error(`${entryKey(source)} archive sha256 mismatch after download. Expected ${source.archiveSha256}, got ${actualSha}`);
  }
  fs.renameSync(tempArchivePath, archivePath);
}

function download(url, destination, redirects = 0) {
  if (redirects > 5) {
    return Promise.reject(new Error(`Too many redirects while downloading ${url}`));
  }

  return new Promise((resolve, reject) => {
    const parsed = new URL(url);
    const client = parsed.protocol === 'http:' ? http : https;
    const request = client.get(
      parsed,
      {
        headers: {
          'User-Agent': 'ai-lemon-tools-proxy-runtime-preparer',
        },
      },
      (response) => {
        const statusCode = response.statusCode ?? 0;
        if (statusCode >= 300 && statusCode < 400 && response.headers.location) {
          response.resume();
          const redirectUrl = new URL(response.headers.location, parsed).toString();
          download(redirectUrl, destination, redirects + 1).then(resolve, reject);
          return;
        }
        if (statusCode !== 200) {
          response.resume();
          reject(new Error(`Download failed with HTTP ${statusCode}: ${url}`));
          return;
        }

        const file = fs.createWriteStream(destination);
        response.pipe(file);
        file.on('finish', () => {
          file.close(resolve);
        });
        file.on('error', reject);
      }
    );
    request.on('error', reject);
  });
}

function extractArchive(archivePath, destination, archiveType, archiveBinaryPath) {
  if (archiveType === 'zip') {
    extractZip(archivePath, destination);
    return;
  }
  if (archiveType === 'tar.gz') {
    runCommand('tar', ['-xzf', archivePath, '-C', destination], `extract ${archivePath}`);
    return;
  }
  if (archiveType === 'gz') {
    extractGzip(archivePath, destination, archiveBinaryPath);
    return;
  }
  throw new Error(`Unsupported archive type: ${archiveType}`);
}

function extractGzip(archivePath, destination, archiveBinaryPath) {
  const outputPath = path.join(destination, ...archiveBinaryPath.split('/'));
  assertInside(destination, outputPath, archiveBinaryPath);
  fs.mkdirSync(path.dirname(outputPath), { recursive: true });
  fs.writeFileSync(outputPath, zlib.gunzipSync(fs.readFileSync(archivePath)));
  chmodExecutable(outputPath);
}

function extractZip(archivePath, destination) {
  if (process.platform === 'win32') {
    const script = [
      "$ErrorActionPreference = 'Stop'",
      `Expand-Archive -LiteralPath ${psQuote(archivePath)} -DestinationPath ${psQuote(destination)} -Force`,
    ].join('\n');
    runCommand(
      'powershell.exe',
      ['-NoProfile', '-ExecutionPolicy', 'Bypass', '-Command', script],
      `extract ${archivePath}`
    );
    return;
  }

  const unzip = spawnSync('unzip', ['-q', archivePath, '-d', destination], {
    encoding: 'utf8',
    stdio: ['ignore', 'pipe', 'pipe'],
  });
  if (unzip.status === 0) {
    return;
  }

  const python = spawnSync('python3', ['-m', 'zipfile', '-e', archivePath, destination], {
    encoding: 'utf8',
    stdio: ['ignore', 'pipe', 'pipe'],
  });
  if (python.status !== 0) {
    throw new Error(
      `Failed to extract zip ${archivePath}. unzip: ${unzip.stderr || unzip.stdout}; python3: ${python.stderr || python.stdout}`
    );
  }
}

function runCommand(command, args, label) {
  const result = spawnSync(command, args, {
    encoding: 'utf8',
    stdio: ['ignore', 'pipe', 'pipe'],
  });
  if (result.status !== 0) {
    throw new Error(`${label} failed: ${result.stderr || result.stdout || `${command} exited ${result.status}`}`);
  }
}

function psQuote(value) {
  return `'${value.replace(/'/g, "''")}'`;
}

function chmodExecutable(filePath) {
  if (process.platform === 'win32') {
    return;
  }
  fs.chmodSync(filePath, 0o755);
}

function buildIndexes(manifest, sources) {
  return {
    manifestByKey: new Map(manifest.files.map((entry) => [entryKey(entry), entry])),
    sourceByKey: new Map(sources.sources.map((source) => [entryKey(source), source])),
  };
}

function selectedEntries(targets, manifestByKey, sourceByKey) {
  const entries = [];
  for (const target of targets) {
    for (const runtime of runtimes) {
      const key = `${target}:${runtime}`;
      const entry = manifestByKey.get(key);
      const source = sourceByKey.get(key);
      if (!entry || !source) {
        throw new Error(`Missing manifest/source entry for ${key}`);
      }
      entries.push({ entry, source });
    }
  }
  return entries;
}

function writeBundle(entries) {
  assertInside(repoRoot, bundleRoot, 'proxy-runtime-bundle');
  fs.rmSync(bundleRoot, { recursive: true, force: true });
  fs.mkdirSync(bundleRoot, { recursive: true });

  const manifestFiles = [];
  for (const { entry } of entries) {
    const sourcePath = runtimePath(entry.path);
    const bundlePath = path.join(bundleRoot, entry.path);
    assertInside(bundleRoot, bundlePath, entry.path);
    fs.mkdirSync(path.dirname(bundlePath), { recursive: true });
    fs.copyFileSync(sourcePath, bundlePath);
    chmodExecutable(bundlePath);
    manifestFiles.push({
      runtime: entry.runtime,
      version: entry.version,
      target: entry.target,
      path: entry.path,
      sha256: entry.preparedSha256 || entry.sha256,
    });
  }

  const bundleManifest = {
    schemaVersion: 1,
    description: 'Proxy runtime binaries bundled with this Tauri build.',
    files: manifestFiles,
  };
  fs.writeFileSync(
    path.join(bundleRoot, 'runtime-manifest.json'),
    `${JSON.stringify(bundleManifest, null, 2)}\n`,
    'utf8'
  );
}

async function main() {
  if (process.env.COCKPIT_SKIP_PROXY_RUNTIME === '1') {
    console.log('[proxy-runtime] skipped by COCKPIT_SKIP_PROXY_RUNTIME=1');
    return;
  }

  const options = parseArgs(process.argv.slice(2));
  const targets = resolveTargets(options);
  const manifest = readJson(runtimeManifestPath);
  const sources = readJson(runtimeSourcesPath);
  validateManifest(manifest, sources);
  const { manifestByKey, sourceByKey } = buildIndexes(manifest, sources);
  const entries = selectedEntries(targets, manifestByKey, sourceByKey);

  for (const { entry, source } of entries) {
    const prepared = await ensureRuntimeFile(entry, source, options);
    entry.preparedSha256 = prepared.sha256;
  }
  writeBundle(entries);

  console.log(`[proxy-runtime] prepared ${entries.length} file(s) for ${targets.join(', ')}`);
  console.log(`[proxy-runtime] bundle: ${bundleRoot}`);
}

main().catch((error) => {
  console.error(`[proxy-runtime] ${error.message}`);
  process.exit(1);
});

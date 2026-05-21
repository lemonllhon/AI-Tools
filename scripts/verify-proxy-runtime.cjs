const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');

const repoRoot = path.resolve(__dirname, '..');
const runtimeRoot = path.join(repoRoot, 'src-tauri', 'proxy-runtime');
const bundleRoot = path.join(repoRoot, 'src-tauri', 'proxy-runtime-bundle');
const runtimeManifestPath = path.join(runtimeRoot, 'runtime-manifest.json');
const runtimeSourcesPath = path.join(runtimeRoot, 'runtime-sources.json');

const supportedTargets = new Set([
  'windows-x86_64',
  'darwin-x86_64',
  'darwin-aarch64',
  'linux-x86_64',
  'linux-aarch64',
]);
const runtimes = new Set(['xray', 'sing-box']);

function usage() {
  return [
    'Usage: node scripts/verify-proxy-runtime.cjs [--targets target[,target...]] [--all] [--bundle]',
    '',
    'Without --bundle, verifies src-tauri/proxy-runtime/bin files.',
    'With --bundle, verifies src-tauri/proxy-runtime-bundle files.',
  ].join('\n');
}

function parseArgs(argv) {
  const options = {
    all: false,
    bundle: false,
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
    if (arg === '--bundle') {
      options.bundle = true;
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

function resolveTargets(options, manifest) {
  let targets = options.targets;
  if (!targets && process.env.PROXY_RUNTIME_TARGETS) {
    targets = splitTargets(process.env.PROXY_RUNTIME_TARGETS);
  }
  if (options.all) {
    targets = Array.from(supportedTargets);
  }
  if (options.bundle && (!targets || targets.length === 0)) {
    targets = Array.from(new Set(manifest.files.map((entry) => entry.target)));
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

function validateManifestShape(manifest) {
  if (manifest.schemaVersion !== 1) {
    throw new Error('runtime-manifest.json schemaVersion must be 1');
  }
  if (!Array.isArray(manifest.files)) {
    throw new Error('runtime-manifest.json files must be an array');
  }

  const seen = new Set();
  for (const entry of manifest.files) {
    if (!supportedTargets.has(entry.target)) {
      throw new Error(`runtime-manifest.json has unsupported target: ${entry.target}`);
    }
    if (!runtimes.has(entry.runtime)) {
      throw new Error(`runtime-manifest.json has unsupported runtime: ${entry.runtime}`);
    }
    if (typeof entry.version !== 'string' || !entry.version) {
      throw new Error(`${entryKey(entry)} version must be a non-empty string`);
    }
    if (typeof entry.path !== 'string' || !entry.path.startsWith(`bin/${entry.target}/`)) {
      throw new Error(`${entryKey(entry)} path must stay under bin/${entry.target}/`);
    }
    assertSha256(entry.sha256, `${entryKey(entry)} sha256`);
    const key = entryKey(entry);
    if (seen.has(key)) {
      throw new Error(`Duplicate manifest entry for ${key}`);
    }
    seen.add(key);
  }
}

function validateSourcesShape(sources, manifest) {
  if (sources.schemaVersion !== 1) {
    throw new Error('runtime-sources.json schemaVersion must be 1');
  }
  if (!Array.isArray(sources.sources)) {
    throw new Error('runtime-sources.json sources must be an array');
  }

  const sourcesByKey = new Map();
  for (const source of sources.sources) {
    if (!supportedTargets.has(source.target)) {
      throw new Error(`runtime-sources.json has unsupported target: ${source.target}`);
    }
    if (!runtimes.has(source.runtime)) {
      throw new Error(`runtime-sources.json has unsupported runtime: ${source.runtime}`);
    }
    if (!['zip', 'tar.gz'].includes(source.archiveType)) {
      throw new Error(`${entryKey(source)} archiveType must be zip or tar.gz`);
    }
    for (const field of ['id', 'version', 'url', 'archiveBinaryPath', 'destPath']) {
      if (typeof source[field] !== 'string' || !source[field]) {
        throw new Error(`${entryKey(source)} source field ${field} must be a non-empty string`);
      }
    }
    assertSha256(source.archiveSha256, `${entryKey(source)} archiveSha256`);
    if (!source.destPath.startsWith(`bin/${source.target}/`)) {
      throw new Error(`${entryKey(source)} destPath must stay under bin/${source.target}/`);
    }
    const key = entryKey(source);
    if (sourcesByKey.has(key)) {
      throw new Error(`Duplicate source entry for ${key}`);
    }
    sourcesByKey.set(key, source);
  }

  for (const entry of manifest.files) {
    const source = sourcesByKey.get(entryKey(entry));
    if (!source) {
      throw new Error(`Missing source entry for ${entryKey(entry)}`);
    }
    if (source.version !== entry.version) {
      throw new Error(`Version mismatch for ${entryKey(entry)}: manifest ${entry.version}, source ${source.version}`);
    }
    if (source.destPath !== entry.path) {
      throw new Error(`Path mismatch for ${entryKey(entry)}: manifest ${entry.path}, source ${source.destPath}`);
    }
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

function verifyFiles(rootDir, manifest, targets) {
  const entries = manifest.files.filter((entry) => targets.includes(entry.target));
  if (entries.length === 0) {
    throw new Error(`No runtime files selected for ${targets.join(', ')}`);
  }

  const expectedKeys = new Set();
  for (const target of targets) {
    for (const runtime of runtimes) {
      expectedKeys.add(`${target}:${runtime}`);
    }
  }

  for (const entry of entries) {
    expectedKeys.delete(entryKey(entry));
    const filePath = path.join(rootDir, entry.path);
    assertInside(rootDir, filePath, entry.path);
    if (!fs.existsSync(filePath)) {
      throw new Error(`${entryKey(entry)} missing file: ${filePath}`);
    }
    const actualSha = sha256File(filePath);
    if (actualSha !== entry.sha256) {
      throw new Error(`${entryKey(entry)} sha256 mismatch. Expected ${entry.sha256}, got ${actualSha}`);
    }
  }

  if (expectedKeys.size > 0) {
    throw new Error(`Missing manifest entries for ${Array.from(expectedKeys).join(', ')}`);
  }
  return entries.length;
}

function main() {
  const options = parseArgs(process.argv.slice(2));
  const manifestPath = options.bundle ? path.join(bundleRoot, 'runtime-manifest.json') : runtimeManifestPath;
  const rootDir = options.bundle ? bundleRoot : runtimeRoot;
  const manifest = readJson(manifestPath);
  validateManifestShape(manifest);

  if (!options.bundle) {
    const sources = readJson(runtimeSourcesPath);
    validateSourcesShape(sources, manifest);
  }

  const targets = resolveTargets(options, manifest);
  const count = verifyFiles(rootDir, manifest, targets);
  console.log(`[proxy-runtime] verified ${count} file(s) for ${targets.join(', ')}`);
}

try {
  main();
} catch (error) {
  console.error(`[proxy-runtime] ${error.message}`);
  process.exit(1);
}

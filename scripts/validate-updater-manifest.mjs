const ownerRepo = process.env.GITHUB_REPOSITORY ?? 'Tuxedogs/siglock';
const [owner, repo] = ownerRepo.split('/');
const endpoint =
  process.env.UPDATER_MANIFEST_URL ??
  `https://github.com/${owner}/${repo}/releases/latest/download/latest.json`;
const expectedVersion =
  process.env.EXPECTED_VERSION ??
  process.argv.find((arg) => arg.startsWith('--version='))?.slice('--version='.length);
const tag =
  process.env.RELEASE_TAG ??
  process.argv.find((arg) => arg.startsWith('--tag='))?.slice('--tag='.length) ??
  (expectedVersion ? `v${expectedVersion.replace(/^v/, '')}` : undefined);
const token = process.env.GITHUB_TOKEN;

function fail(message) {
  console.error(`Updater validation failed: ${message}`);
  process.exit(1);
}

async function fetchOk(url, options = {}) {
  let lastStatus = 'no response';
  for (let attempt = 1; attempt <= 5; attempt += 1) {
    const response = await fetch(url, {
      ...options,
      headers: {
        'User-Agent': 'siglock-updater-validation',
        Accept: 'application/vnd.github+json',
        ...(token ? { Authorization: `Bearer ${token}` } : {}),
        ...(options.headers ?? {}),
      },
    });
    if (response.ok) return response;
    lastStatus = String(response.status);
    if (attempt < 5) {
      await new Promise((resolve) => setTimeout(resolve, attempt * 3000));
    }
  }
  fail(`${url} returned ${lastStatus}`);
}

function assetIdFromUrl(url) {
  return /\/releases\/assets\/(\d+)(?:$|[/?#])/.exec(url)?.[1];
}

if (!owner || !repo) fail('GITHUB_REPOSITORY must be in owner/repo form');
if (!expectedVersion) fail('set EXPECTED_VERSION or pass --version=x.y.z');
if (!tag) fail('set RELEASE_TAG or pass --tag=vx.y.z');

const manifestResponse = await fetchOk(endpoint, {
  headers: { Accept: 'application/json' },
});
const manifest = await manifestResponse.json();

if (manifest.version !== expectedVersion && manifest.version !== `v${expectedVersion}`) {
  fail(`manifest version ${manifest.version} does not match ${expectedVersion}`);
}

const platform =
  manifest.platforms?.['windows-x86_64'] ??
  manifest.platforms?.['windows-x86_64-nsis'];

if (!platform?.url) fail('manifest is missing platforms.windows-x86_64.url');
if (!platform?.signature) fail('manifest is missing platforms.windows-x86_64.signature');

const installerResponse = await fetchOk(platform.url, {
  headers: { Accept: 'application/octet-stream' },
});
await installerResponse.body?.cancel();

const releaseResponse = await fetchOk(
  `https://api.github.com/repos/${owner}/${repo}/releases/tags/${tag}`,
);
const release = await releaseResponse.json();

if (release.draft) fail(`${tag} is still a draft release`);
if (release.prerelease) {
  fail(`${tag} is marked as a prerelease; GitHub /releases/latest ignores prereleases`);
}

const assets = Array.isArray(release.assets) ? release.assets : [];
const latestJson = assets.find((asset) => asset.name === 'latest.json');
if (!latestJson) fail(`${tag} does not include latest.json`);

const installerAssetId = assetIdFromUrl(platform.url);
const installerAsset = installerAssetId
  ? assets.find((asset) => String(asset.id) === installerAssetId)
  : assets.find((asset) => asset.browser_download_url === platform.url);

if (!installerAsset) fail('manifest installer URL does not reference a release asset');
if (!installerAsset.name.endsWith('_x64-setup.exe')) {
  fail(`manifest URL points to unexpected installer asset ${installerAsset.name}`);
}

const signatureAsset = assets.find((asset) => asset.name === `${installerAsset.name}.sig`);
if (!signatureAsset) fail(`missing signature asset ${installerAsset.name}.sig`);

const signatureResponse = await fetchOk(signatureAsset.browser_download_url, {
  headers: { Accept: 'application/octet-stream' },
});
const signature = (await signatureResponse.text()).trim();

if (signature !== String(platform.signature).trim()) {
  fail('manifest signature does not match the uploaded .sig asset');
}

console.log(`Updater manifest OK for ${tag}: ${manifest.version}`);

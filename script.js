'use strict';

const motionPreference = matchMedia('(prefers-reduced-motion: reduce)');
const tau = 2 * Math.PI;
const topAngle = -Math.PI / 2;

const icons = {
  files: ['#ff9e47', 'M2.5 7V5.5Q2.5 4.5 3.5 4.5H9L11 7H20Q21.5 7 21.5 8.5V18Q21.5 19.5 20 19.5H4Q2.5 19.5 2.5 18ZM2.5 10H21.5'],
  apps: ['#ffc75c', 'M4 3.5H9Q10 3.5 10 4.5V9Q10 10 9 10H4Q3 10 3 9V4.5Q3 3.5 4 3.5ZM15 3.5H20Q21 3.5 21 4.5V9Q21 10 20 10H15Q14 10 14 9V4.5Q14 3.5 15 3.5ZM4 14H9Q10 14 10 15V19.5Q10 20.5 9 20.5H4Q3 20.5 3 19.5V15Q3 14 4 14ZM15 14H20Q21 14 21 15V19.5Q21 20.5 20 20.5H15Q14 20.5 14 19.5V15Q14 14 15 14Z'],
  windows: ['#ff7a57', 'M7.5 6V4.5Q7.5 3 9 3H19.5Q21 3 21 4.5V14Q21 15.5 19.5 15.5H18M3 11H16.5M4.5 7.5H15Q16.5 7.5 16.5 9V19.5Q16.5 21 15 21H4.5Q3 21 3 19.5V9Q3 7.5 4.5 7.5Z'],
  wifi: ['#5cd6b3', 'M2.5 8.5Q12 1 21.5 8.5M5.5 12Q12 6.8 18.5 12M8.5 15.5Q12 12.7 15.5 15.5M12 19H12.01'],
  bluetooth: ['#6ba8ff', 'M6 6.5 17.5 17 12 22V2L17.5 7 6 17.5'],
  clipboard: ['#5cd6b3', 'M8 5H6a2 2 0 0 0-2 2v12a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7a2 2 0 0 0-2-2h-2M8 4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2H8ZM8 11h8m-8 4h8'],
  terminal: ['#eea1a5', 'M4 4H20Q21 4 21 5V19Q21 20 20 20H4Q3 20 3 19V5Q3 4 4 4ZM7 9 10 12 7 15M13 15H17'],
  web: ['#8abbff', 'M22 12A10 10 0 1 1 2 12 10 10 0 0 1 22 12ZM2 12H22M12 2C6 8 6 16 12 22M12 2C18 8 18 16 12 22'],
  text: ['#ffd494', 'M5 2.5H14L19 7.5V20Q19 21.5 17.5 21.5H6.5Q5 21.5 5 20ZM14 2.5V7.5H19M8.5 12H15.5M8.5 16H15.5'],
  picture: ['#9ad5b4', 'M4 4H20Q21 4 21 5V19Q21 20 20 20H4Q3 20 3 19V5Q3 4 4 4ZM3 16 8 11 13 16 16 13 21 18M16 8H16.01'],
  settings: ['#c9b2ff', 'M4 6H20M4 12H20M4 18H20M9 4V8M15 10V14M8 16V20'],
  custom: ['#ffb86b', 'M21 12A9 9 0 1 1 3 12 9 9 0 0 1 21 12ZM8 12H16M12 8V16'],
  audio: ['#ff9a82', 'M4 14V12A8 8 0 0 1 20 12V14M4 13H7V20H5Q4 20 4 19ZM20 13H17V20H19Q20 20 20 19Z'],
  clock: ['#8fdcc7', 'M22 12A10 10 0 1 1 2 12 10 10 0 0 1 22 12ZM12 6V12L16 15'],
};

function createIcon(name) {
  const [color, path] = icons[name];
  const canvas = document.createElement('canvas');
  canvas.width = canvas.height = 72;
  canvas.setAttribute('aria-hidden', 'true');
  const context = canvas.getContext('2d');
  context.scale(3, 3);
  context.lineWidth = 1.8;
  context.lineJoin = context.lineCap = 'round';
  context.strokeStyle = color;
  context.stroke(new Path2D(path));
  return canvas;
}

document.querySelectorAll('[data-icon]').forEach(element => {
  element.append(createIcon(element.dataset.icon));
});

const platformSwitch = document.querySelector('#platform-switch');
const platformChoices = platformSwitch.querySelectorAll('input');
const platformContent = document.querySelectorAll('[data-platform-content]');
const platformFeatures = document.querySelector('#platform-features');

function selectPlatform(platform) {
  platformChoices.forEach(choice => { choice.checked = choice.value === platform; });
  platformContent.forEach(section => { section.hidden = section.dataset.platformContent !== platform; });
  platformFeatures.dataset.platform = platform;
}

const browserPlatform = navigator.userAgentData?.platform || navigator.platform;
selectPlatform(/^win/i.test(browserPlatform) ? 'windows' : 'linux');
platformSwitch.hidden = false;
platformSwitch.addEventListener('change', event => {
  selectPlatform(event.target.value);
});

// Retarget the value and velocity already on screen, even mid-transition.
class Spring {
  constructor(value, wobble = false) {
    this.value = this.target = value;
    this.velocity = 0;
    this.wobble = wobble;
  }

  snap() {
    this.value = this.target;
    this.velocity = 0;
  }

  advance(dt) {
    if (motionPreference.matches) {
      this.snap();
      return false;
    }

    const offset = this.value - this.target;
    if (Math.abs(offset) < 0.001 && Math.abs(this.velocity) < 0.02) {
      this.snap();
      return false;
    }

    if (this.wobble) {
      const slope = (this.velocity + 15 * offset) / 21;
      const sine = Math.sin(21 * dt);
      const cosine = Math.cos(21 * dt);
      const decay = Math.exp(-15 * dt);
      const wave = offset * cosine + slope * sine;
      this.value = this.target + decay * wave;
      this.velocity = decay * (21 * (slope * cosine - offset * sine) - 15 * wave);
    } else {
      const frequency = 23;
      const slope = this.velocity + frequency * offset;
      const decay = Math.exp(-frequency * dt);
      this.value = this.target + decay * (offset + slope * dt);
      this.velocity = decay * (this.velocity - frequency * slope * dt);
    }
    return true;
  }
}

const apps = [
  ['Web Browser', 'web'], ['Files', 'files'], ['Terminal', 'terminal'], ['Editor', 'text'],
  ['Photo Viewer', 'picture'], ['Settings', 'settings'], ['Music', 'audio'], ['Clock', 'clock'],
  ['File Archive', 'files'], ['Workspaces', 'windows'], ['File Search', 'files'], ['Text Files', 'text'],
];
const search = document.querySelector('#app-search');
const results = document.querySelector('#layout-results');
const layoutField = document.querySelector('#layout-field');
const layoutStatus = document.querySelector('#layout-status');
const noResults = document.querySelector('#no-results');
let matches = [];

const appItems = apps.map(([name, icon]) => {
  const element = document.createElement('li');
  element.className = 'app-result';
  const glyph = createIcon(icon);
  const label = document.createElement('span');
  label.textContent = name;
  element.append(glyph, label);
  results.append(element);
  return { name, element, glyph, x: new Spring(0), y: new Spring(0), opacity: new Spring(0), tilt: new Spring(0, true) };
});

function arrangeApps(snap = false) {
  const ring = matches.length > 0 && matches.length <= 8;
  const layoutChanged = layoutField.hasAttribute('data-ring') !== ring;
  const { clientWidth: width, clientHeight: height } = layoutField;
  layoutField.toggleAttribute('data-ring', ring);
  const diameter = Math.max(0, Math.min(width, height) - 12);
  layoutField.style.setProperty('--ring-diameter', `${diameter}px`);
  const radius = diameter * .285;
  const rowCount = Math.ceil(matches.length / 4);
  const rowHeight = height / 3;

  for (const item of appItems) {
    const index = matches.indexOf(item);
    const visible = index >= 0;
    item.element.setAttribute('aria-hidden', String(!visible));
    item.opacity.target = Number(visible);
    if (visible) {
      // An eight-slot layout for seven results keeps the lower labels apart.
      const slot = index + Number(matches.length === 7 && index >= 4);
      const angle = topAngle + slot * tau / (matches.length === 7 ? 8 : matches.length);
      const rowStart = Math.floor(index / 4) * 4;
      const rowSize = Math.min(4, matches.length - rowStart);
      item.x.target = ring ? width / 2 + Math.cos(angle) * radius : width / 2 + (index % 4 - (rowSize - 1) / 2) * width / 4;
      item.y.target = ring ? height / 2 + Math.sin(angle) * radius : height / 2 + (Math.floor(index / 4) - (rowCount - 1) / 2) * rowHeight;
      if (layoutChanged && !snap) item.tilt.velocity += index % 2 ? 3 : -3;
    } else {
      item.x.target = width / 2;
      item.y.target = height / 2;
    }
    if (snap) {
      item.x.snap();
      item.y.snap();
      item.opacity.snap();
    }
    item.element.toggleAttribute('data-sliding', !snap && (item.x.value !== item.x.target || item.y.value !== item.y.target));
  }

  noResults.hidden = matches.length > 0;
  schedule();
}

function filterApps(snap = false) {
  const query = search.value.trim().toLowerCase();
  matches = appItems.filter(item => item.name.toLowerCase().includes(query));
  const label = matches.length === 1 ? 'app' : 'apps';
  const layout = matches.length ? (matches.length > 8 ? ' · grid layout' : ' · ring layout') : '';
  layoutStatus.textContent = `${matches.length} sample ${label}${layout}`;
  arrangeApps(snap);
}

function drawApps(dt) {
  let moving = false;
  for (const item of appItems) {
    const xMoving = item.x.advance(dt);
    const yMoving = item.y.advance(dt);
    const fading = item.opacity.advance(dt);
    const tilting = item.tilt.advance(dt);
    moving = moving || xMoving || yMoving || fading || tilting;
    item.element.style.transform = `translate3d(${item.x.value}px, ${item.y.value}px, 0) translate(-50%, -50%)`;
    item.element.style.opacity = item.opacity.value;
    item.element.hidden = !item.opacity.value;
    const distanceToTarget = Math.hypot(item.x.target - item.x.value, item.y.target - item.y.value);
    item.element.toggleAttribute('data-sliding', distanceToTarget > 1);
    item.glyph.style.transform = `rotate(${item.tilt.value}rad)`;
  }
  return moving;
}

let frame = 0;
let lastFrame = 0;

function schedule() {
  if (!frame) frame = requestAnimationFrame(animate);
}

function animate(now) {
  frame = 0;
  const dt = lastFrame ? Math.min((now - lastFrame) / 1000, .064) : 1 / 60;
  lastFrame = drawApps(dt) ? now : 0;
  if (lastFrame) schedule();
}

filterApps(true);
new ResizeObserver(() => arrangeApps(true)).observe(layoutField);
motionPreference.addEventListener('change', schedule);
search.disabled = false;
search.addEventListener('input', () => filterApps());
search.addEventListener('keydown', event => {
  if (event.key === 'Escape' && search.value) {
    event.preventDefault();
    search.value = '';
    filterApps();
  }
});

const windowsDownload = document.querySelector('#windows-download');
const windowsStatus = document.querySelector('#windows-download-status');
const windowsReleases = 'https://api.github.com/repos/RekoSK/ReCircles-windows/releases';
let checkingWindowsRelease = false;

windowsDownload.addEventListener('click', async event => {
  if (event.ctrlKey || event.metaKey || event.shiftKey || event.altKey) return;
  event.preventDefault();
  if (checkingWindowsRelease) return;

  checkingWindowsRelease = true;
  windowsStatus.hidden = false;
  windowsStatus.textContent = 'Checking for the newest Windows installer…';

  let downloadUrl = windowsDownload.href;
  try {
    const signal = AbortSignal.timeout(5000);
    const stableResponse = await fetch(`${windowsReleases}/latest`, { signal });
    let releases;
    if (stableResponse.ok) {
      releases = [await stableResponse.json()];
    } else if (stableResponse.status === 404) {
      // A missing stable release is the only reason to consider prereleases.
      const betaResponse = await fetch(windowsReleases, { signal });
      if (!betaResponse.ok) throw new Error('GitHub prerelease lookup failed');

      releases = (await betaResponse.json())
        .filter(release => release.prerelease && !release.draft && release.published_at)
        .sort((a, b) => Date.parse(b.published_at) - Date.parse(a.published_at));
    } else {
      throw new Error('GitHub stable release lookup failed');
    }

    const installer = releases
      .flatMap(release => release.assets)
      .find(asset => asset.state === 'uploaded'
        && /^recircles-setup(?:-[\w.-]+)?\.exe$/i.test(asset.name)
        && asset.browser_download_url.startsWith('https://github.com/RekoSK/ReCircles-windows/releases/download/'));
    if (!installer) throw new Error('No Windows installer found');

    downloadUrl = installer.browser_download_url;
    windowsStatus.hidden = true;
  } catch {
    windowsStatus.textContent = 'Opening the Windows releases page…';
  } finally {
    checkingWindowsRelease = false;
  }

  window.location.assign(downloadUrl);
});

document.querySelectorAll('[data-copy-commands]').forEach(copyButton => {
  const block = copyButton.closest('.code-block');
  const installCommands = block.querySelector('code');
  const copyStatus = block.querySelector('[role="status"]');
  let resetCopy;
  copyButton.hidden = false;
  copyButton.addEventListener('click', async () => {
    let copied = false;
    try {
      await navigator.clipboard.writeText(installCommands.textContent);
      copied = true;
    } catch {
      const range = document.createRange();
      range.selectNodeContents(installCommands);
      const selection = window.getSelection();
      selection.removeAllRanges();
      selection.addRange(range);
      copied = document.execCommand('copy');
      if (copied) selection.removeAllRanges();
    }

    copyButton.textContent = copied ? 'Copied' : 'Commands selected';
    copyStatus.textContent = copied ? 'Commands copied.' : 'Commands selected. Use your copy shortcut.';
    clearTimeout(resetCopy);
    resetCopy = setTimeout(() => { copyButton.textContent = 'Copy commands'; }, 2300);
  });
});

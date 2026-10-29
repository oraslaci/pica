import { writable } from 'svelte/store';
import { getAppVersion, openExternalUrl, restartApp, isTauri } from './api';
import type { ReleaseAsset, ReleaseInfo, UpdateCheckResult } from './types';

const GITHUB_REPO = 'oraslaci/pica';
const GITHUB_RELEASES_API = `https://api.github.com/repos/${GITHUB_REPO}/releases/latest`;

const STORAGE_KEY_AUTO_CHECK = 'pica_update_auto_check';
const STORAGE_KEY_LAST_CHECKED = 'pica_update_last_checked';
const STORAGE_KEY_DISMISSED = 'pica_update_dismissed_version';

export function cleanVersion(v: string): string {
  return v.replace(/^[vV]/, '').trim();
}

/**
 * Returns:
 *   1 if v1 > v2
 *  -1 if v1 < v2
 *   0 if v1 == v2
 */
export function compareSemver(v1: string, v2: string): number {
  const s1 = cleanVersion(v1);
  const s2 = cleanVersion(v2);

  const parts1 = s1.split(/[.-]/).map((p) => {
    const num = parseInt(p, 10);
    return isNaN(num) ? p : num;
  });
  const parts2 = s2.split(/[.-]/).map((p) => {
    const num = parseInt(p, 10);
    return isNaN(num) ? p : num;
  });

  const maxLen = Math.max(parts1.length, parts2.length);
  for (let i = 0; i < maxLen; i++) {
    const p1 = parts1[i] ?? 0;
    const p2 = parts2[i] ?? 0;

    if (typeof p1 === 'number' && typeof p2 === 'number') {
      if (p1 > p2) return 1;
      if (p1 < p2) return -1;
    } else {
      const cmp = String(p1).localeCompare(String(p2));
      if (cmp !== 0) return cmp;
    }
  }

  return 0;
}

export function isNewerVersion(current: string, remote: string): boolean {
  return compareSemver(remote, current) > 0;
}

export function detectUserPlatform(): 'linux' | 'windows' | 'macos' | 'unknown' {
  if (typeof navigator === 'undefined') return 'unknown';
  const ua = navigator.userAgent.toLowerCase();
  const platform = (navigator.platform || '').toLowerCase();

  if (ua.includes('linux') || platform.includes('linux')) return 'linux';
  if (ua.includes('mac') || platform.includes('mac') || ua.includes('darwin')) return 'macos';
  if (ua.includes('win') || platform.includes('win')) return 'windows';
  return 'unknown';
}

export function formatBytes(bytes: number): string {
  if (!bytes || bytes === 0) return '0 B';
  const k = 1024;
  const sizes = ['B', 'KB', 'MB', 'GB'];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return parseFloat((bytes / Math.pow(k, i)).toFixed(1)) + ' ' + sizes[i];
}

export function formatRelativeDate(isoStr: string): string {
  try {
    const date = new Date(isoStr);
    const now = new Date();
    const diffSec = Math.floor((now.getTime() - date.getTime()) / 1000);

    if (diffSec < 60) return 'Just now';
    if (diffSec < 3600) return `${Math.floor(diffSec / 60)} minutes ago`;
    if (diffSec < 86400) return `${Math.floor(diffSec / 3600)} hours ago`;
    if (diffSec < 604800) {
      const days = Math.floor(diffSec / 86400);
      return days === 1 ? 'Yesterday' : `${days} days ago`;
    }
    return date.toLocaleDateString(undefined, {
      year: 'numeric',
      month: 'short',
      day: 'numeric',
    });
  } catch {
    return isoStr;
  }
}

export function getRecommendedAsset(
  assets: ReleaseAsset[],
  platform = detectUserPlatform()
): ReleaseAsset | null {
  if (!assets || assets.length === 0) return null;

  if (platform === 'linux') {
    return (
      assets.find((a) => a.name.toLowerCase().endsWith('.appimage')) ||
      assets.find((a) => a.name.toLowerCase().endsWith('.deb') && !a.name.includes('arm')) ||
      assets.find((a) => a.name.toLowerCase().endsWith('.deb')) ||
      assets.find((a) => a.name.toLowerCase().endsWith('.rpm')) ||
      assets[0]
    );
  }

  if (platform === 'windows') {
    return (
      assets.find((a) => a.name.toLowerCase().endsWith('-setup.exe')) ||
      assets.find((a) => a.name.toLowerCase().endsWith('.exe')) ||
      assets.find((a) => a.name.toLowerCase().endsWith('.msi')) ||
      assets[0]
    );
  }

  if (platform === 'macos') {
    return (
      assets.find((a) => a.name.toLowerCase().endsWith('.dmg')) ||
      assets.find((a) => a.name.toLowerCase().endsWith('.tar.gz')) ||
      assets.find((a) => a.name.toLowerCase().endsWith('.zip')) ||
      assets[0]
    );
  }

  return assets[0] || null;
}

export interface UpdateState {
  isChecking: boolean;
  updateAvailable: boolean;
  currentVersion: string;
  latestVersion: string;
  release: ReleaseInfo | null;
  recommendedAsset: ReleaseAsset | null;
  lastChecked: number | null;
  error: string | null;
  isModalOpen: boolean;
  autoCheckEnabled: boolean;
  dismissedVersion: string | null;
  // In-place self update states
  canSelfUpdate: boolean;
  isSelfUpdating: boolean;
  selfUpdateProgress: number; // 0 to 100
  selfUpdateStatusText: string;
  isReadyToRestart: boolean;
}

const initialAutoCheck =
  typeof window !== 'undefined'
    ? localStorage.getItem(STORAGE_KEY_AUTO_CHECK) !== 'false'
    : true;

const initialLastChecked =
  typeof window !== 'undefined' && localStorage.getItem(STORAGE_KEY_LAST_CHECKED)
    ? parseInt(localStorage.getItem(STORAGE_KEY_LAST_CHECKED) || '0', 10)
    : null;

const initialDismissed =
  typeof window !== 'undefined'
    ? localStorage.getItem(STORAGE_KEY_DISMISSED)
    : null;

const defaultState: UpdateState = {
  isChecking: false,
  updateAvailable: false,
  currentVersion: '0.2.1',
  latestVersion: '0.2.1',
  release: null,
  recommendedAsset: null,
  lastChecked: initialLastChecked,
  error: null,
  isModalOpen: false,
  autoCheckEnabled: initialAutoCheck,
  dismissedVersion: initialDismissed,
  canSelfUpdate: isTauri,
  isSelfUpdating: false,
  selfUpdateProgress: 0,
  selfUpdateStatusText: '',
  isReadyToRestart: false,
};

let tauriUpdateHandle: any = null;

function createUpdateStore() {
  const { subscribe, set, update } = writable<UpdateState>(defaultState);

  // Initialize current version asynchronously from backend
  if (typeof window !== 'undefined') {
    getAppVersion().then((ver) => {
      update((s) => ({
        ...s,
        currentVersion: ver,
        latestVersion: s.release ? s.latestVersion : ver,
      }));
    });
  }

  async function checkForUpdates(force = false): Promise<UpdateCheckResult> {
    let currentVer = '0.2.1';
    try {
      currentVer = await getAppVersion();
    } catch {
      // Use fallback
    }

    update((s) => ({
      ...s,
      isChecking: true,
      error: null,
      currentVersion: currentVer,
    }));

    // First attempt: check via official Tauri in-place updater if in desktop app
    if (isTauri) {
      try {
        const { check } = await import('@tauri-apps/plugin-updater');
        const tauriUpdate = await check();
        if (tauriUpdate) {
          tauriUpdateHandle = tauriUpdate;
          const remoteVer = cleanVersion(tauriUpdate.version);
          const hasUpdate = isNewerVersion(currentVer, remoteVer);
          const now = Date.now();

          const release: ReleaseInfo = {
            version: remoteVer,
            tag_name: `v${remoteVer}`,
            name: `Pica v${remoteVer}`,
            body: tauriUpdate.body || '',
            published_at: tauriUpdate.date || new Date().toISOString(),
            html_url: `https://github.com/${GITHUB_REPO}/releases/tag/v${remoteVer}`,
            assets: [],
          };

          if (typeof window !== 'undefined') {
            try {
              localStorage.setItem(STORAGE_KEY_LAST_CHECKED, String(now));
            } catch {}
          }

          const isDismissed = !force && localStorage.getItem(STORAGE_KEY_DISMISSED) === remoteVer;

          update((s) => ({
            ...s,
            isChecking: false,
            updateAvailable: hasUpdate && !isDismissed,
            currentVersion: currentVer,
            latestVersion: remoteVer,
            release,
            recommendedAsset: null,
            canSelfUpdate: true,
            lastChecked: now,
            error: null,
          }));

          return {
            has_update: hasUpdate,
            current_version: currentVer,
            latest_version: remoteVer,
            release,
            checked_at: now,
            error: null,
          };
        }
      } catch (e) {
        console.info('Tauri in-place updater check fell back to GitHub API:', e);
      }
    }

    // Second attempt / fallback: GitHub Releases API
    try {
      const response = await fetch(GITHUB_RELEASES_API, {
        headers: {
          Accept: 'application/vnd.github.v3+json',
        },
      });

      if (!response.ok) {
        throw new Error(`GitHub API returned status ${response.status}`);
      }

      const data = await response.json();
      const tagName = data.tag_name || '';
      const remoteVer = cleanVersion(tagName || data.name || '');

      const release: ReleaseInfo = {
        version: remoteVer,
        tag_name: tagName,
        name: data.name || tagName,
        body: data.body || '',
        published_at: data.published_at || new Date().toISOString(),
        html_url: data.html_url || `https://github.com/${GITHUB_REPO}/releases`,
        assets: (data.assets || []).map((a: any) => ({
          name: a.name,
          size: a.size,
          browser_download_url: a.browser_download_url,
          content_type: a.content_type,
        })),
      };

      const hasUpdate = isNewerVersion(currentVer, remoteVer);
      const recommended = getRecommendedAsset(release.assets);
      const now = Date.now();

      if (typeof window !== 'undefined') {
        try {
          localStorage.setItem(STORAGE_KEY_LAST_CHECKED, String(now));
        } catch {}
      }

      const isDismissed = !force && localStorage.getItem(STORAGE_KEY_DISMISSED) === remoteVer;

      update((s) => ({
        ...s,
        isChecking: false,
        updateAvailable: hasUpdate && !isDismissed,
        currentVersion: currentVer,
        latestVersion: remoteVer,
        release,
        recommendedAsset: recommended,
        canSelfUpdate: tauriUpdateHandle !== null,
        lastChecked: now,
        error: null,
      }));

      return {
        has_update: hasUpdate,
        current_version: currentVer,
        latest_version: remoteVer,
        release,
        checked_at: now,
        error: null,
      };
    } catch (err: any) {
      const errorMsg = err?.message || 'Failed to check for updates';
      update((s) => ({
        ...s,
        isChecking: false,
        error: errorMsg,
      }));
      return {
        has_update: false,
        current_version: currentVer,
        latest_version: currentVer,
        release: null,
        checked_at: Date.now(),
        error: errorMsg,
      };
    }
  }

  function openModal() {
    update((s) => ({ ...s, isModalOpen: true }));
  }

  function closeModal() {
    update((s) => ({ ...s, isModalOpen: false }));
  }

  function dismissUpdate(version: string) {
    if (typeof window !== 'undefined') {
      try {
        localStorage.setItem(STORAGE_KEY_DISMISSED, version);
      } catch {}
    }
    update((s) => ({
      ...s,
      dismissedVersion: version,
      updateAvailable: false,
      isModalOpen: false,
    }));
  }

  function setAutoCheck(enabled: boolean) {
    if (typeof window !== 'undefined') {
      try {
        localStorage.setItem(STORAGE_KEY_AUTO_CHECK, String(enabled));
      } catch {}
    }
    update((s) => ({ ...s, autoCheckEnabled: enabled }));
  }

  async function downloadUpdate(targetUrl?: string) {
    let urlToOpen = targetUrl;
    let fallbackReleaseUrl = `https://github.com/${GITHUB_REPO}/releases`;

    update((s) => {
      if (!urlToOpen) {
        urlToOpen = s.recommendedAsset?.browser_download_url || s.release?.html_url || fallbackReleaseUrl;
      }
      return s;
    });

    if (urlToOpen) {
      await openExternalUrl(urlToOpen);
    }
  }

  async function startSelfUpdate() {
    if (!tauriUpdateHandle) {
      // If tauri updater handle isn't present, download installer
      await downloadUpdate();
      return;
    }

    update((s) => ({
      ...s,
      isSelfUpdating: true,
      selfUpdateProgress: 0,
      selfUpdateStatusText: 'Connecting to update server...',
      error: null,
    }));

    try {
      let downloaded = 0;
      let total = 0;

      await tauriUpdateHandle.downloadAndInstall((event: any) => {
        if (event.event === 'Started') {
          total = event.data?.contentLength || 0;
          update((s) => ({
            ...s,
            selfUpdateStatusText: 'Starting download...',
          }));
        } else if (event.event === 'Progress') {
          downloaded += event.data?.chunkLength || 0;
          const pct = total > 0 ? Math.round((downloaded / total) * 100) : 0;
          update((s) => ({
            ...s,
            selfUpdateProgress: pct,
            selfUpdateStatusText: total > 0
              ? `Downloading update (${pct}%) — ${formatBytes(downloaded)} / ${formatBytes(total)}`
              : `Downloading update — ${formatBytes(downloaded)}`,
          }));
        } else if (event.event === 'Finished') {
          update((s) => ({
            ...s,
            selfUpdateProgress: 100,
            isReadyToRestart: true,
            isSelfUpdating: false,
            selfUpdateStatusText: 'Update installed successfully! Restart to apply.',
          }));
        }
      });

      update((s) => ({
        ...s,
        isReadyToRestart: true,
        isSelfUpdating: false,
        selfUpdateStatusText: 'Update installed! Restart to apply changes.',
      }));
    } catch (err: any) {
      const errorMsg = err?.message || String(err);
      console.error('Self-update error:', err);
      update((s) => ({
        ...s,
        isSelfUpdating: false,
        error: `In-place self-update failed: ${errorMsg}. You can download the package directly below.`,
      }));
    }
  }

  async function restartApplication() {
    await restartApp();
  }

  function simulateUpdate(mockVersion = '0.3.0', withSelfUpdate = true) {
    const mockRelease: ReleaseInfo = {
      version: mockVersion,
      tag_name: `v${mockVersion}`,
      name: `Pica v${mockVersion} — Modern Update System & Performance Boost`,
      body: `### What's New in Pica v${mockVersion}\n\n- ✨ **In-Place Self Updating**: Direct seamless binary update and restart.\n- 🚀 **Face Recognition Speedup**: 2x faster landmark alignment and responsive medoid clustering.\n- 🎨 **Darkroom HUD Polish**: Sleeker floating glass controls and high-contrast badges.\n- 🛡️ **Non-Destructive Originals**: Smarter backup preservation and conflict resolution.\n\nThank you for using Pica!`,
      published_at: new Date().toISOString(),
      html_url: `https://github.com/${GITHUB_REPO}/releases/tag/v${mockVersion}`,
      assets: [
        {
          name: `pica_${mockVersion}_amd64.AppImage`,
          size: 28420000,
          browser_download_url: `https://github.com/${GITHUB_REPO}/releases/download/v${mockVersion}/pica_${mockVersion}_amd64.AppImage`,
          content_type: 'application/octet-stream',
        },
        {
          name: `pica_${mockVersion}_amd64.deb`,
          size: 27950000,
          browser_download_url: `https://github.com/${GITHUB_REPO}/releases/download/v${mockVersion}/pica_${mockVersion}_amd64.deb`,
          content_type: 'application/vnd.debian.binary-package',
        },
        {
          name: `pica-${mockVersion}-1.x86_64.rpm`,
          size: 28220000,
          browser_download_url: `https://github.com/${GITHUB_REPO}/releases/download/v${mockVersion}/pica-${mockVersion}-1.x86_64.rpm`,
          content_type: 'application/x-rpm',
        },
        {
          name: `pica_${mockVersion}_x64-setup.exe`,
          size: 21600000,
          browser_download_url: `https://github.com/${GITHUB_REPO}/releases/download/v${mockVersion}/pica_${mockVersion}_x64-setup.exe`,
          content_type: 'application/x-msdos-program',
        },
        {
          name: `pica_${mockVersion}_x64.dmg`,
          size: 31200000,
          browser_download_url: `https://github.com/${GITHUB_REPO}/releases/download/v${mockVersion}/pica_${mockVersion}_x64.dmg`,
          content_type: 'application/x-apple-diskimage',
        },
      ],
    };

    update((s) => ({
      ...s,
      updateAvailable: true,
      latestVersion: mockVersion,
      release: mockRelease,
      recommendedAsset: getRecommendedAsset(mockRelease.assets),
      canSelfUpdate: withSelfUpdate,
      isSelfUpdating: false,
      isReadyToRestart: false,
      selfUpdateProgress: 0,
      selfUpdateStatusText: '',
      error: null,
    }));
  }

  function simulateSelfUpdateProgress() {
    update((s) => ({
      ...s,
      isSelfUpdating: true,
      selfUpdateProgress: 10,
      selfUpdateStatusText: 'Downloading update package... 10%',
      isReadyToRestart: false,
    }));

    let progress = 10;
    const interval = setInterval(() => {
      progress += 25;
      if (progress >= 100) {
        clearInterval(interval);
        update((s) => ({
          ...s,
          isSelfUpdating: false,
          selfUpdateProgress: 100,
          selfUpdateStatusText: 'Update installed! Ready to restart.',
          isReadyToRestart: true,
        }));
      } else {
        update((s) => ({
          ...s,
          selfUpdateProgress: progress,
          selfUpdateStatusText: `Downloading & installing... ${progress}%`,
        }));
      }
    }, 400);
  }

  function resetUpdateState() {
    tauriUpdateHandle = null;
    update((s) => ({
      ...s,
      updateAvailable: false,
      latestVersion: s.currentVersion,
      release: null,
      recommendedAsset: null,
      error: null,
      dismissedVersion: null,
      isSelfUpdating: false,
      isReadyToRestart: false,
      selfUpdateProgress: 0,
      selfUpdateStatusText: '',
    }));
    if (typeof window !== 'undefined') {
      try {
        localStorage.removeItem(STORAGE_KEY_DISMISSED);
      } catch {}
    }
  }

  return {
    subscribe,
    checkForUpdates,
    openModal,
    closeModal,
    dismissUpdate,
    setAutoCheck,
    downloadUpdate,
    startSelfUpdate,
    restartApplication,
    simulateUpdate,
    simulateSelfUpdateProgress,
    resetUpdateState,
  };
}

export const updateStore = createUpdateStore();

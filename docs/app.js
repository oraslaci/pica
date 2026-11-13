/**
 * Pica — Fast, Free & Private Offline Desktop Photo Manager
 * Landing Page Client Script
 */

document.addEventListener('DOMContentLoaded', () => {
  initMobileMenu();
  initBeforeAfterSlider();
  initOsDetectionAndTabs();
});

/**
 * Mobile Navigation Toggle
 */
function initMobileMenu() {
  const menuBtn = document.querySelector('.mobile-menu-btn');
  const mainNav = document.querySelector('.main-nav');

  if (!menuBtn || !mainNav) return;

  menuBtn.addEventListener('click', () => {
    const isExpanded = menuBtn.getAttribute('aria-expanded') === 'true';
    menuBtn.setAttribute('aria-expanded', !isExpanded);
    mainNav.classList.toggle('mobile-open');
  });

  // Close menu when clicking nav links
  mainNav.querySelectorAll('.nav-link').forEach(link => {
    link.addEventListener('click', () => {
      mainNav.classList.remove('mobile-open');
      menuBtn.setAttribute('aria-expanded', 'false');
    });
  });
}

/**
 * Interactive Before / After Image Slider
 */
function initBeforeAfterSlider() {
  const wrapper = document.querySelector('.slider-wrapper');
  const afterImage = document.querySelector('.slider-image-after');
  const handle = document.querySelector('.slider-handle');

  if (!wrapper || !afterImage || !handle) return;

  let isDragging = false;

  const updatePosition = (clientX) => {
    const rect = wrapper.getBoundingClientRect();
    let x = clientX - rect.left;
    
    // Clamp to 5% - 95%
    const min = rect.width * 0.05;
    const max = rect.width * 0.95;
    x = Math.max(min, Math.min(x, max));

    const percentage = (x / rect.width) * 100;
    
    afterImage.style.clipPath = `polygon(0 0, ${percentage}% 0, ${percentage}% 100%, 0 100%)`;
    handle.style.left = `${percentage}%`;
  };

  // Mouse events
  wrapper.addEventListener('mousedown', (e) => {
    isDragging = true;
    updatePosition(e.clientX);
  });

  window.addEventListener('mousemove', (e) => {
    if (!isDragging) return;
    updatePosition(e.clientX);
  });

  window.addEventListener('mouseup', () => {
    isDragging = false;
  });

  // Touch events (Mobile support)
  wrapper.addEventListener('touchstart', (e) => {
    isDragging = true;
    if (e.touches.length > 0) {
      updatePosition(e.touches[0].clientX);
    }
  }, { passive: true });

  window.addEventListener('touchmove', (e) => {
    if (!isDragging) return;
    if (e.touches.length > 0) {
      updatePosition(e.touches[0].clientX);
    }
  }, { passive: true });

  window.addEventListener('touchend', () => {
    isDragging = false;
  });

  // Keyboard accessibility
  handle.setAttribute('tabindex', '0');
  handle.setAttribute('role', 'slider');
  handle.setAttribute('aria-label', 'Before and After photo comparison slider');
  handle.setAttribute('aria-valuenow', '50');
  handle.setAttribute('aria-valuemin', '5');
  handle.setAttribute('aria-valuemax', '95');

  handle.addEventListener('keydown', (e) => {
    const currentLeft = parseFloat(handle.style.left || '50');
    let newLeft = currentLeft;

    if (e.key === 'ArrowLeft' || e.key === 'ArrowDown') {
      newLeft = Math.max(5, currentLeft - 5);
      e.preventDefault();
    } else if (e.key === 'ArrowRight' || e.key === 'ArrowUp') {
      newLeft = Math.min(95, currentLeft + 5);
      e.preventDefault();
    }

    if (newLeft !== currentLeft) {
      afterImage.style.clipPath = `polygon(0 0, ${newLeft}% 0, ${newLeft}% 100%, 0 100%)`;
      handle.style.left = `${newLeft}%`;
      handle.setAttribute('aria-valuenow', Math.round(newLeft).toString());
    }
  });
}

/**
 * OS Auto-Detection & Download Tab Switcher
 */
function initOsDetectionAndTabs() {
  const tabs = document.querySelectorAll('.tab-btn');
  const panels = document.querySelectorAll('.os-download-panel');
  const heroDownloadBtn = document.querySelector('#hero-download-btn');

  if (!tabs.length || !panels.length) return;

  // Detect User Operating System
  let detectedOs = 'windows'; // default fallback
  const userAgent = window.navigator.userAgent.toLowerCase();
  const platform = (window.navigator.userAgentData?.platform || window.navigator.platform || '').toLowerCase();

  if (platform.includes('mac') || userAgent.includes('macintosh') || userAgent.includes('mac os')) {
    detectedOs = 'macos';
  } else if (platform.includes('linux') || userAgent.includes('linux') || userAgent.includes('x11')) {
    detectedOs = 'linux';
  } else if (platform.includes('win') || userAgent.includes('windows')) {
    detectedOs = 'windows';
  }

  // Update Hero Download Button text & link
  if (heroDownloadBtn) {
    if (detectedOs === 'macos') {
      heroDownloadBtn.textContent = 'Download for macOS (Free)';
    } else if (detectedOs === 'linux') {
      heroDownloadBtn.textContent = 'Download for Linux (Free)';
    } else {
      heroDownloadBtn.textContent = 'Download for Windows (Free)';
    }
  }

  // Switch to detected OS tab
  activateOsTab(detectedOs);

  // Tab click events
  tabs.forEach(tab => {
    tab.addEventListener('click', () => {
      const targetOs = tab.getAttribute('data-os');
      if (targetOs) {
        activateOsTab(targetOs);
      }
    });
  });

  function activateOsTab(osName) {
    tabs.forEach(tab => {
      const isTarget = tab.getAttribute('data-os') === osName;
      tab.classList.toggle('active', isTarget);
      tab.setAttribute('aria-selected', isTarget ? 'true' : 'false');
    });

    panels.forEach(panel => {
      const isTarget = panel.getAttribute('data-os-panel') === osName;
      panel.classList.toggle('active', isTarget);
    });
  }
}

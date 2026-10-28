import { writable } from 'svelte/store';

const STORAGE_KEY = 'pica_debug_edit_overlay';

const initialValue =
  typeof window !== 'undefined'
    ? localStorage.getItem(STORAGE_KEY) === 'true'
    : false;

export const showEditOverlay = writable<boolean>(initialValue);

const FACE_STORAGE_KEY = 'pica_debug_face_overlay';

const initialFaceValue =
  typeof window !== 'undefined'
    ? localStorage.getItem(FACE_STORAGE_KEY) === 'true'
    : false;

export const showFaceDebugOverlay = writable<boolean>(initialFaceValue);

if (typeof window !== 'undefined') {
  showEditOverlay.subscribe((val) => {
    try {
      localStorage.setItem(STORAGE_KEY, String(val));
    } catch {}
  });

  showFaceDebugOverlay.subscribe((val) => {
    try {
      localStorage.setItem(FACE_STORAGE_KEY, String(val));
    } catch {}
  });
}

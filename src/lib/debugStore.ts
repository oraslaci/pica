import { writable } from 'svelte/store';

const STORAGE_KEY = 'pica_debug_edit_overlay';

const initialValue =
  typeof window !== 'undefined'
    ? localStorage.getItem(STORAGE_KEY) === 'true'
    : false;

export const showEditOverlay = writable<boolean>(initialValue);

if (typeof window !== 'undefined') {
  showEditOverlay.subscribe((val) => {
    try {
      localStorage.setItem(STORAGE_KEY, String(val));
    } catch {}
  });
}

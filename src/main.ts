import { mount } from 'svelte'
import './app.css'
import App from './App.svelte'
import PicaPhotoViewer from './lib/components/PicaPhotoViewer.svelte'

const params = new URLSearchParams(window.location.search);
const isViewerMode = params.get('mode') === 'viewer' || params.has('viewer');
const initialFilePath = params.get('file') || params.get('path') || undefined;

const app = isViewerMode
  ? mount(PicaPhotoViewer, {
      target: document.getElementById('app')!,
      props: { initialFilePath },
    })
  : mount(App, {
      target: document.getElementById('app')!,
    });

export default app

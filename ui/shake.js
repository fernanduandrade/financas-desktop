'use strict';
/* Tremor: a janela (transparente até a foto carregar) cobre a tela com uma foto dela e sacode por ~0,5 s. */
const T = window.__TAURI__;
const img = document.getElementById('shot');
const done = () => T.core.invoke('shake_done');
(async () => {
  const { amp } = await T.core.invoke('shake_info');
  img.onerror = done;
  img.onload = () => {
    document.body.classList.add('ready');
    const frames = 30, t0 = performance.now();
    const step = now => {
      const i = Math.floor((now - t0) / 16);
      if (i >= frames) { img.style.transform = 'none'; return done(); }
      const decay = Math.pow(1 - i / frames, 1.6);
      const dx = amp * decay * Math.sin(i * 2.4), dy = amp * 0.45 * decay * Math.cos(i * 3.3);
      img.style.transform = `translate(${dx}px, ${dy}px)`;
      requestAnimationFrame(step);
    };
    requestAnimationFrame(step);
  };
  img.src = T.core.convertFileSrc(`tela-${Date.now()}.jpg`, 'shot');
})().catch(done);

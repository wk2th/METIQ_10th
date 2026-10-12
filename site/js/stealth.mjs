// stealth.mjs — "if you capture it, it was always just a 1996 page."
//
// Every effect already decays to nothing within ~2s of stillness. On top of
// that we watch for the usual precursors of a capture and slam everything to
// rest *synchronously*, inside the event, before the compositor's next frame:
//
//   PrintScreen, ⌘⇧3/4/5 (macOS), ⊞+Shift+S (Windows), Ctrl+Shift (ChromeOS),
//   window blur (OS snipping overlays steal focus), tab hidden, print.

export function armStealth(onCalm) {
  const hit = (why) => onCalm(why);

  addEventListener(
    'keydown',
    (e) => {
      if (
        e.key === 'PrintScreen' ||
        e.key === 'Meta' ||
        e.key === 'OS' ||
        (e.shiftKey && (e.metaKey || e.ctrlKey))
      )
        hit('key');
    },
    { capture: true },
  );
  addEventListener('keyup', (e) => e.key === 'PrintScreen' && hit('key'), { capture: true });
  addEventListener('blur', () => hit('blur'));
  addEventListener('beforeprint', () => hit('print'));
  document.addEventListener('visibilitychange', () => document.hidden && hit('hidden'));
}

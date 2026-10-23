/* Read-only Canvas2D call observation. The renderer and its algorithms are
 * unchanged; only calls on the visible graph canvas are retained. */
(() => {
  const canvas = document.querySelector('#graph');
  const target = canvas.getContext('2d');
  const proto = CanvasRenderingContext2D.prototype;
  window.auditPaint = { labels: [], lines: [], arrows: [] };
  let path = [], curved = false;
  const observe = (name, before) => {
    const original = proto[name];
    proto[name] = function (...args) {
      if (this === target) before.apply(this, args);
      return original.apply(this, args);
    };
  };
  observe('clearRect', () => {
    window.auditPaint = { labels: [], lines: [], arrows: [] };
  });
  observe('beginPath', () => { path = []; curved = false; });
  observe('moveTo', (x, y) => path.push(['M', x, y]));
  observe('lineTo', (x, y) => path.push(['L', x, y]));
  observe('arc', () => { curved = true; });
  observe('stroke', function () {
    if (!curved && path.length) window.auditPaint.lines.push({
      width: this.lineWidth, color: this.strokeStyle, path: path.slice()
    });
  });
  observe('fill', function () {
    if (!curved && path.length) window.auditPaint.arrows.push({
      color: this.fillStyle, path: path.slice()
    });
  });
  observe('drawImage', function (image, x, y, w, h) {
    if (image instanceof HTMLCanvasElement && Number.isFinite(w)) {
      const font = image.getContext('2d').font;
      const match = /([\d.]+)px/.exec(font);
      window.auditPaint.labels.push({
        alpha: this.globalAlpha, font, bitmap_font_size: match ? +match[1] : null,
        bitmap_size: [image.width, image.height], x, y, width: w, height: h
      });
    }
  });
  // Capture frame callbacks without changing their normal execution until an
  // explicit controlled-step scenario temporarily owns the queue.
  const request = window.requestAnimationFrame.bind(window);
  const cancel = window.cancelAnimationFrame.bind(window);
  let held = false, serial = 1000000;
  const pending = new Map();
  window.requestAnimationFrame = callback => {
    if (!held) return request(callback);
    const id = ++serial; pending.set(id, callback); return id;
  };
  window.cancelAnimationFrame = id => {
    if (!pending.delete(id)) cancel(id);
  };
  window.auditFrames = {
    hold() { held = true; },
    step(time) {
      const callbacks = [...pending.values()]; pending.clear();
      callbacks.forEach(callback => callback(time));
      return callbacks.length;
    },
    release() {
      held = false;
      const callbacks = [...pending.values()]; pending.clear();
      callbacks.forEach(callback => request(callback));
    }
  };
  return true;
})()

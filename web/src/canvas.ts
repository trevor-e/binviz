// Small canvas helpers shared by the strips and grids that draw the file.

export function cssVar(name: string): string {
  return getComputedStyle(document.documentElement).getPropertyValue(name).trim();
}

/** Sizes a canvas for the device pixel ratio and returns a context in CSS pixels. */
export function setup(canvas: HTMLCanvasElement, w: number, hgt: number): CanvasRenderingContext2D {
  const dpr = window.devicePixelRatio || 1;
  canvas.width = Math.round(w * dpr);
  canvas.height = Math.round(hgt * dpr);
  canvas.style.height = `${hgt}px`;
  const g = canvas.getContext('2d')!;
  g.scale(dpr, dpr);
  g.clearRect(0, 0, w, hgt);
  return g;
}

export function roundRect(g: CanvasRenderingContext2D, x: number, y: number, w: number, hgt: number, r: number) {
  g.beginPath();
  g.roundRect(x, y, w, hgt, r);
  g.fill();
}

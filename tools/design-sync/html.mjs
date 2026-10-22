export function normalizeHtml(input) {
  const normalized = input.replace(/color\(srgb\s+([^)]+)\)/g, (_, values) => {
    const [channels, alpha] = values.split('/');
    const rgb = channels.trim().split(/\s+/).map(value => Math.round(Number(value) * 255));
    return `rgba(${rgb.join(',')},${alpha === undefined ? 1 : Number(alpha.trim())})`;
  });
  const html = normalized.replace(/<svg\b([^>]*)>([\s\S]*?)<\/svg>/gi, (original, attributes, body) => {
    const viewBox = attributes.match(/viewBox="([^"]+)"/i);
    if (!viewBox) return original;
    const [x, y, width, height] = viewBox[1].split(/[ ,]+/).map(Number);
    const outputWidth = Number(attributes.match(/(?:^|\s)width="([^"]+)"/)?.[1]);
    const outputHeight = Number(attributes.match(/(?:^|\s)height="([^"]+)"/)?.[1]);
    if (!width || !height || !outputWidth || !outputHeight) return original;
    let sx = outputWidth / width;
    let sy = outputHeight / height;
    let dx = 0;
    let dy = 0;
    if (!/preserveAspectRatio="none"/i.test(attributes)) {
      sx = sy = Math.min(sx, sy);
      dx = (outputWidth - width * sx) / 2;
      dy = (outputHeight - height * sy) / 2;
    }
    const strokeScale = (sx + sy) / 2;
    if (/preserveAspectRatio="none"/i.test(attributes)) body = body.replace(/stroke-width:([0-9.]+)px/g, (_, width) => `stroke-width:${Number(width) / strokeScale}px`);
    body = body.replace(/<[^>]+vector-effect="non-scaling-stroke"[^>]*>/g, tag => tag.replace(/stroke-width:([0-9.]+)px/g, (_, width) => `stroke-width:${Number(width) / strokeScale}px`));
    return `<svg ${attributes.replace(/\s*viewBox="[^"]+"/i, '')}><g transform="matrix(${sx} 0 0 ${sy} ${dx - x * sx} ${dy - y * sy})">${body}</g></svg>`;
  });
  return html;
}

({ name, mainSelector = "main" }) => {
  const escape = value => String(value).replaceAll('&', '&amp;').replaceAll('<', '&lt;').replaceAll('>', '&gt;').replaceAll('"', '&quot;');
  const px = value => `${Math.round(value * 100) / 100}px`;
  const number = value => +value.replace(/[^0-9.\-]/g, '');
  const position = (rect, parent) => `position:absolute;left:${px(rect.left - parent.left)};top:${px(rect.top - parent.top)};width:${px(rect.width)};height:${px(rect.height)};`;
  let count = 0;
  const anchors = {};
  const classes = element => [...element.classList].filter(c => !/^svelte-|^xyflow__|^selected$|^dragging$/.test(c)).sort();
  const key = element => {
    if (element.id) return `#${CSS.escape(element.id)}`;
    const bits = [];
    for (let at = element; at && at !== document.body; at = at.parentElement) {
      const test = at.getAttribute('data-testid');
      const part = test ? `[data-testid="${CSS.escape(test)}"]` : `${at.tagName.toLowerCase()}${classes(at).map(c => '.' + CSS.escape(c)).join('')}:nth-child(${[...at.parentElement.children].indexOf(at) + 1})`;
      bits.unshift(part);
      if (test || at.tagName === 'MAIN') break;
    }
    return bits.join(' > ');
  };
  const token = (element, label, text = null, suffix = '') => {
    const id = `__sync_${Object.keys(anchors).length}`;
    const heading = [...element.children].find(c => /^H[1-6]$/.test(c.tagName))?.textContent.trim().slice(0, 100) || '';
    const descendantIdentity = text === null ? element.textContent.replace(/\s+/g, ' ').trim() : null;
    const visualIdentity = text === null ? [element, ...element.querySelectorAll('img, svg')].filter(e => ['IMG','svg'].includes(e.tagName)).map(e => e.tagName === 'IMG' ? e.getAttribute('src') : e.innerHTML) : null;
    const signature = JSON.stringify([element.tagName, classes(element), element.getAttribute('role'), element.getAttribute('aria-label'), heading, text, descendantIdentity, visualIdentity]);
    const rect = element.getBoundingClientRect();
    anchors[id] = { key: key(element) + suffix, signature, label, text, selector: key(element), tag: element.tagName.toLowerCase(),
      bounds: { x: rect.left, y: rect.top, width: rect.width, height: rect.height } };
    return id;
  };

  function svgChild(element) {
    const computed = getComputedStyle(element);
    const attributes = [...element.attributes].filter(attribute => !['style', 'class'].includes(attribute.name)).map(attribute => `${attribute.name}="${escape(attribute.value)}"`).join(' ');
    const style = ['fill', 'fill-opacity', 'stroke', 'stroke-opacity', 'stroke-width', 'stroke-linecap', 'stroke-linejoin', 'stroke-dasharray', 'vector-effect', 'opacity', 'font-size', 'font-weight', 'text-anchor'].map(property => `${property}:${computed.getPropertyValue(property)}`).join(';') + ';font-family:Inter';
    return `<${element.tagName} ${attributes} style="${escape(style)}">${[...element.childNodes].map(node => node.nodeType === 3 ? escape(node.textContent) : node.nodeType === 1 ? svgChild(node) : '').join('')}</${element.tagName}>`;
  }

  function textLines(node, computed, parent, scale) {
    const text = node.textContent;
    const range = document.createRange();
    const lines = [];
    for (let index = 0; index < text.length; index++) {
      range.setStart(node, index);
      range.setEnd(node, index + 1);
      const rect = range.getBoundingClientRect();
      if (rect.width < 0.01 || rect.height < 0.01) continue;
      const last = lines[lines.length - 1];
      if (last && Math.abs(last.top - rect.top) < 2) {
        last.text += text[index];
        last.right = Math.max(last.right, rect.right);
      } else lines.push({ text: text[index], left: rect.left, right: rect.right, top: rect.top, height: rect.height });
    }
    const fontSize = number(computed.fontSize) * scale;
    const lineHeight = (number(computed.lineHeight) || number(computed.fontSize) * 1.2) * scale;
    return lines.filter(line => line.text.trim()).map((line, lineIndex) => {
      let content = line.text.replace(/\s+/g, ' ').trim();
      if (computed.textTransform === 'uppercase') content = content.toUpperCase();
      const rect = { left: line.left, top: line.top - (lineHeight - fontSize) / 2, width: (line.right - line.left) * 1.35 + 8, height: lineHeight + 2 };
      const tracking = computed.letterSpacing === 'normal' ? '0px' : px(number(computed.letterSpacing) * scale);
      const style = position(rect, parent) + `font-size:${px(fontSize)};font-weight:${computed.fontWeight};font-style:${computed.fontStyle};line-height:${px(lineHeight)};letter-spacing:${tracking};color:${computed.color};white-space:pre;`;
      return `<span layer-name="${token(node.parentElement, content.slice(0, 70), content, `::text(${[...node.parentElement.childNodes].indexOf(node)}):line(${lineIndex})`)}" style="${style}">${escape(content)}</span>`;
    }).join('');
  }

  function render(element, parent) {
    const computed = getComputedStyle(element);
    const rect = element.getBoundingClientRect();
    const scale = element.offsetWidth > 0 ? rect.width / element.offsetWidth : 1;
    const closedDetails = element.closest('details:not([open])');
    if (closedDetails && element !== closedDetails && !element.closest('summary')) return '';
    if (computed.display === 'none' || computed.visibility === 'hidden' || +computed.opacity === 0 || ((rect.width < 0.1 || rect.height < 0.1) && !element.children.length) || element.hidden || element.classList.contains('sr-only') || ['SCRIPT', 'STYLE', 'DIALOG', 'TITLE'].includes(element.tagName)) return '';
    count++;
    const label = element.getAttribute('aria-label') || (element.classList.contains('svelte-flow__node') ? element.querySelector('[aria-label^="Inspect "]')?.getAttribute('aria-label') : '') || element.id || element.classList[0] || element.tagName.toLowerCase();
    if (element.tagName.toLowerCase() === 'svg') {
      let attributes = [...element.attributes].filter(attribute => !['style', 'class', 'width', 'height', 'x', 'y'].includes(attribute.name)).map(attribute => `${attribute.name}="${escape(attribute.value)}"`).join(' ');
      if (!element.hasAttribute('viewBox')) attributes += ` viewBox="0 0 ${number(computed.width) || 300} ${number(computed.height) || 150}"`;
      return `<svg ${attributes} width="${rect.width}" height="${rect.height}" layer-name="${token(element, label)}" style="${position(rect, parent)}">${[...element.children].map(svgChild).join('')}</svg>`;
    }
    let style = position(rect, parent) + 'display:flex;';
    if (computed.backgroundColor !== 'rgba(0, 0, 0, 0)') style += `background-color:${computed.backgroundColor};`;
    if (number(computed.opacity) < 1) style += `opacity:${computed.opacity};`;
    style += `border-radius:${computed.borderRadius};`;
    if (['hidden', 'clip', 'auto', 'scroll'].includes(computed.overflowX) || ['hidden', 'clip', 'auto', 'scroll'].includes(computed.overflowY)) style += 'overflow:hidden;';
    let children = '';
    const borders = ['top', 'right', 'bottom', 'left'].map(side => ({ side, width: number(computed.getPropertyValue(`border-${side}-width`)), color: computed.getPropertyValue(`border-${side}-color`), type: computed.getPropertyValue(`border-${side}-style`) })).filter(border => border.width > 0 && border.type !== 'none');
    if (borders.length === 4 && borders.every(border => border.width === borders[0].width && border.color === borders[0].color)) style += `border:${px(borders[0].width)} solid ${borders[0].color};`;
    else for (const border of borders) {
      const vertical = border.side === 'left' || border.side === 'right';
      const left = border.side === 'right' ? rect.width - border.width : 0;
      const top = border.side === 'bottom' ? rect.height - border.width : 0;
      children += `<div layer-name="${border.side} divider" style="position:absolute;left:${px(left)};top:${px(top)};width:${px(vertical ? border.width : rect.width)};height:${px(vertical ? rect.height : border.width)};background-color:${border.color};"></div>`;
    }
    for (const node of element.childNodes) {
      if (node.nodeType === 1) children += render(node, rect);
      else if (node.nodeType === 3 && node.textContent.trim()) children += textLines(node, computed, rect, scale);
    }
    if (element.tagName === 'INPUT' && element.type === 'range') {
      const fraction = (+element.value - +element.min) / (+element.max - +element.min);
      children += `<div layer-name="Slider track" style="position:absolute;left:0px;top:${px(rect.height / 2 - 2)};width:${px(rect.width)};height:4px;background-color:#DFE5ED;border-radius:2px;"></div><div layer-name="Slider progress" style="position:absolute;left:0px;top:${px(rect.height / 2 - 2)};width:${px(rect.width * fraction)};height:4px;background-color:#4A8CCF;border-radius:2px;"></div><div layer-name="Slider thumb" style="position:absolute;left:${px(rect.width * fraction - 7)};top:${px(rect.height / 2 - 7)};width:14px;height:14px;background-color:#4A8CCF;border-radius:7px;"></div>`;
    }
    if (element.tagName === 'SELECT') children += `<span layer-name="Selected option" style="position:absolute;left:8px;top:4px;width:${px(rect.width)};height:${px(rect.height)};font-size:${computed.fontSize};color:${computed.color};">${escape(element.selectedOptions[0]?.textContent || '')}</span>`;
    return `<div layer-name="${token(element, label)}" style="${style}">${children}</div>`;
  }
  const root = { left: 0, top: 0 };
  const main = document.querySelector(mainSelector) || document.querySelector('main');
  const sections = [document.querySelector('aside.sidebar'), document.querySelector('.topbar'), ...main.children].filter(Boolean);
  const mainRect = main.getBoundingClientRect();
  const mainBackground = getComputedStyle(main).backgroundColor;
  const backdrop = { name: 'Page background', html: `<div layer-name="Page background" style="${position(mainRect, root)}background-color:${mainBackground};"></div>` };
  const result = { name, width: innerWidth, height: Math.max(document.documentElement.scrollHeight, innerHeight), anchors, sections: [backdrop, ...sections.map((element, index) => ({ name: element.getAttribute('aria-label') || element.querySelector('h2')?.textContent || element.classList[0] || `Section ${index + 1}`, html: render(element, root) }))], count };
  const signatures = new Map();
  for (const anchor of Object.values(anchors)) signatures.set(anchor.signature, (signatures.get(anchor.signature) || 0) + 1);
  for (const anchor of Object.values(anchors)) anchor.ambiguous = signatures.get(anchor.signature) > 1;
  return result;
}

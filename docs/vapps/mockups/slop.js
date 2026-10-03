// Slop mascot generator — deterministic from a DNA string. Mirrors the idea of
// packages/icons: a fixed registry of parts, composed per client. 2.5D look via
// gradients + a highlight + a soft shadow. No server work, no model.
(function (global) {
  const BODIES = {
    bean:   "M100,30 C150,25 175,70 170,115 C165,165 125,185 90,180 C45,175 25,135 30,95 C35,55 60,33 100,30 Z",
    pear:   "M100,28 C125,28 128,60 135,80 C160,100 170,130 160,155 C148,182 60,182 42,155 C30,130 42,100 65,80 C72,60 75,28 100,28 Z",
    cloud:  "M70,70 C70,45 110,38 118,62 C140,50 170,70 160,95 C180,105 178,140 150,150 C145,175 100,180 85,160 C50,172 25,140 42,115 C25,95 45,70 70,70 Z",
    pill:   "M60,40 L140,40 C165,40 180,60 180,85 L180,125 C180,150 165,170 140,170 L60,170 C35,170 20,150 20,125 L20,85 C20,60 35,40 60,40 Z",
    blob:   "M95,32 C140,22 178,60 172,105 C168,140 150,178 105,178 C60,178 22,150 28,100 C32,62 60,38 95,32 Z",
    square: "M55,35 L145,35 C165,35 175,45 175,65 L175,145 C175,165 165,175 145,175 L55,175 C35,175 25,165 25,145 L25,65 C25,45 35,35 55,35 Z",
    heart:  "M100,175 C60,145 20,115 22,78 C24,50 50,35 72,42 C85,46 95,56 100,66 C105,56 115,46 128,42 C150,35 176,50 178,78 C180,115 140,145 100,175 Z",
    drop:   "M100,28 C120,70 165,100 165,135 C165,165 135,182 100,182 C65,182 35,165 35,135 C35,100 80,70 100,28 Z",
    flower: "M100,32 C118,32 128,48 124,62 C140,55 158,65 158,82 C158,96 148,104 136,106 C148,112 152,130 142,142 C132,154 116,150 108,142 C108,160 94,172 80,166 C66,160 62,146 68,134 C50,142 32,130 34,112 C36,98 48,92 60,94 C50,82 54,62 70,58 C80,56 90,62 92,70 C88,52 86,32 100,32 Z",
    star:   "M100,30 L118,72 L165,78 L132,110 L140,158 L100,136 L60,158 L68,110 L35,78 L82,72 Z",
  };
  const BODY_KEYS = Object.keys(BODIES);
  const PALETTE = [
    ["#F87171", "#C2410C"], ["#FB923C", "#B45309"], ["#FACC15", "#A16207"], ["#4ADE80", "#15803D"],
    ["#2DD4BF", "#0F766E"], ["#60A5FA", "#1D4ED8"], ["#A78BFA", "#6D28D9"], ["#F472B6", "#BE185D"],
  ];
  const EYES = ["round", "diamond", "happy", "dot", "wide", "sleepy"];
  const EXTRAS = ["none", "beret", "bowtie", "glasses", "monocle", "sunglasses", "sprout", "antenna"];
  const MOUTHS = ["smile", "o", "flat", "cat"];

  function hash(s) { let h = 2166136261; for (const c of s) { h ^= c.charCodeAt(0); h = Math.imul(h, 16777619) >>> 0; } return h; }
  function fromSeed(seed) {
    const h = hash(String(seed));
    return {
      body: BODY_KEYS[h % BODY_KEYS.length],
      hue: (h >>> 4) % PALETTE.length,
      eyes: EYES[(h >>> 8) % EYES.length],
      extra: EXTRAS[(h >>> 12) % EXTRAS.length],
      mouth: MOUTHS[(h >>> 16) % MOUTHS.length],
      tilt: ((h >>> 20) % 13) - 6,
    };
  }
  function eyesSvg(kind, ink) {
    const L = [78, 98], R = [122, 98];
    const one = ([x, y]) => {
      switch (kind) {
        case "diamond": return `<path d="M${x},${y - 9} L${x + 7},${y} L${x},${y + 9} L${x - 7},${y} Z" fill="${ink}"/>`;
        case "happy": return `<path d="M${x - 8},${y + 3} Q${x},${y - 8} ${x + 8},${y + 3}" stroke="${ink}" stroke-width="4" fill="none" stroke-linecap="round"/>`;
        case "dot": return `<circle cx="${x}" cy="${y}" r="3.5" fill="${ink}"/>`;
        case "wide": return `<ellipse cx="${x}" cy="${y}" rx="8" ry="10" fill="#fff"/><circle cx="${x + 1}" cy="${y + 2}" r="5" fill="${ink}"/><circle cx="${x + 3}" cy="${y - 1}" r="1.6" fill="#fff"/>`;
        case "sleepy": return `<path d="M${x - 8},${y - 2} Q${x},${y + 6} ${x + 8},${y - 2}" stroke="${ink}" stroke-width="4" fill="none" stroke-linecap="round"/>`;
        default: return `<circle cx="${x}" cy="${y}" r="6.5" fill="${ink}"/><circle cx="${x + 2.2}" cy="${y - 2.2}" r="2" fill="#fff"/>`;
      }
    };
    return one(L) + one(R);
  }
  function mouthSvg(kind, ink) {
    switch (kind) {
      case "o": return `<ellipse cx="100" cy="122" rx="5" ry="6" fill="${ink}"/>`;
      case "flat": return `<path d="M91,122 L109,122" stroke="${ink}" stroke-width="3.5" stroke-linecap="round"/>`;
      case "cat": return `<path d="M90,119 Q95,126 100,120 Q105,126 110,119" stroke="${ink}" stroke-width="3.5" fill="none" stroke-linecap="round"/>`;
      default: return `<path d="M88,118 Q100,132 112,118" stroke="${ink}" stroke-width="3.5" fill="none" stroke-linecap="round"/>`;
    }
  }
  function extraSvg(kind, ink, dark) {
    switch (kind) {
      case "beret": return `<path d="M58,52 C70,28 130,24 146,48 C150,56 140,58 100,56 C62,58 54,60 58,52 Z" fill="${dark}"/><circle cx="104" cy="32" r="4" fill="${dark}"/>`;
      case "bowtie": return `<path d="M100,150 L80,140 L80,160 Z M100,150 L120,140 L120,160 Z" fill="${dark}"/><circle cx="100" cy="150" r="4" fill="${ink}"/>`;
      case "glasses": return `<circle cx="78" cy="98" r="15" stroke="${ink}" stroke-width="3" fill="rgba(255,255,255,.12)"/><circle cx="122" cy="98" r="15" stroke="${ink}" stroke-width="3" fill="rgba(255,255,255,.12)"/><path d="M93,98 L107,98" stroke="${ink}" stroke-width="3"/>`;
      case "monocle": return `<circle cx="122" cy="98" r="15" stroke="${ink}" stroke-width="3" fill="rgba(255,255,255,.12)"/><path d="M134,108 Q140,130 132,145" stroke="${ink}" stroke-width="2.5" fill="none"/>`;
      case "sunglasses": return `<rect x="62" y="88" width="32" height="20" rx="8" fill="${ink}"/><rect x="106" y="88" width="32" height="20" rx="8" fill="${ink}"/><path d="M94,96 L106,96" stroke="${ink}" stroke-width="3"/>`;
      case "sprout": return `<path d="M100,34 C100,20 110,12 122,14 C118,26 110,32 100,34 Z" fill="#4ADE80"/><path d="M100,34 C98,24 90,16 80,18 C84,28 92,34 100,34 Z" fill="#22C55E"/>`;
      case "antenna": return `<path d="M100,32 L100,14" stroke="${dark}" stroke-width="3.5" stroke-linecap="round"/><circle cx="100" cy="12" r="5" fill="#FACC15"/>`;
      default: return "";
    }
  }
  function svg(dna, size = 160, opts = {}) {
    const d = typeof dna === "string" ? fromSeed(dna) : dna;
    const [c1, c2] = PALETTE[d.hue];
    const ink = "#0b0b0e";
    const id = "g" + hash(JSON.stringify(d)).toString(36);
    const busy = opts.busy ? `<g class="slop-busy">` : `<g>`;
    return `<svg viewBox="0 0 200 200" width="${size}" height="${size}" xmlns="http://www.w3.org/2000/svg" class="slop" style="overflow:visible">
<defs>
  <radialGradient id="${id}" cx="38%" cy="30%" r="80%"><stop offset="0" stop-color="#fff" stop-opacity=".55"/><stop offset=".35" stop-color="${c1}"/><stop offset="1" stop-color="${c2}"/></radialGradient>
  <filter id="${id}s" x="-30%" y="-30%" width="160%" height="170%"><feDropShadow dx="0" dy="10" stdDeviation="8" flood-color="${c2}" flood-opacity=".45"/></filter>
</defs>
${busy}<g transform="rotate(${d.tilt} 100 110)">
  <path d="${BODIES[d.body]}" fill="${c1}" filter="url(#${id}s)"/>
  <path d="${BODIES[d.body]}" fill="url(#${id})"/>
  <path d="${BODIES[d.body]}" fill="none" stroke="#fff" stroke-opacity=".25" stroke-width="2"/>
  <ellipse cx="74" cy="58" rx="11" ry="6" fill="#fff" fill-opacity=".55" transform="rotate(-20 74 58)"/>
  ${eyesSvg(d.eyes, ink)}
  ${mouthSvg(d.mouth, ink)}
  ${extraSvg(d.extra, ink, c2)}
</g></g></svg>`;
  }
  global.Slop = { fromSeed, svg, BODY_KEYS, PALETTE, EYES, EXTRAS, MOUTHS };
})(typeof window !== "undefined" ? window : globalThis);

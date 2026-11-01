Motion animation JSON (compiles to Lottie). Root: {"size":[w,h], "duration":ms, "fps":60, "layers":[…]}. Layers are listed TOP first; each draws in the composition's pixel space (origin top-left).

Layer: {"type":"shape", "id":"name", "transform":{…}, "shapes":[…]}. Other types: "solid" {color, size}, "null" (a parent for others), "precomp" {asset, size} with "assets":{"id":{"layers":[…]}}, "text" {text:{text, font, size, color, align}}. Optional: "parent":"id", "in"/"out" (ms), "blend":"multiply"|"screen"…, "matte":{"source":"id","mode":"alpha"|"alpha-inverted"|"luma"|"luma-inverted"}, "masks":[{"path":"M…Z","mode":"add"|"subtract"|"intersect","inverted":true}].

transform (layers and groups): anchor [x,y], position [x,y], scale [100,100] (percent), rotation (degrees, clockwise), opacity (0–100), skew, skewAxis.

shapes, drawn top first, a style paints every shape listed BEFORE it in the same group:
- geometry: {"type":"rect","position":[x,y],"size":[w,h],"radius":r} (position = centre), {"type":"ellipse","position","size"}, {"type":"star","points":5,"outerRadius","innerRadius"}, {"type":"polygon","points","outerRadius"}, {"type":"path","d":"M0 0 L10 0 C… Z"} (SVG path data);
- styles: {"type":"fill","color"}, {"type":"stroke","color","width","cap":"round","join":"round","dash":[len,gap]}, {"type":"gradient-fill","kind":"linear"|"radial","start":[x,y],"end":[x,y],"stops":[[0,"#fff"],[1,"#000"]]};
- modifiers: {"type":"trim","start":0,"end":100,"offset":0} (percent; draw-on lines), {"type":"repeater","copies":n,"transform":{…}}, {"type":"round-corners","radius"};
- {"type":"group","shapes":[…],"transform":{…}} nests and transforms its own shapes.
Put shapes inside a group when they need their own transform.

Colours: "#rrggbb", "#rrggbbaa", a theme token "$color.primary" (also foreground, muted, accent, destructive, success, warning, info, chart1…chart8) that follows light/dark mode, or {"light":"#000","dark":"#fff"}. Prefer tokens.

Animate ANY value with keys: {"keys":[{"t":0,"v":[0,0],"ease":"ease-in-out"},{"t":600,"v":[100,0]}]}. t in ms, v the value at t, ease (for the segment that starts at this key): linear (default), ease, ease-in, ease-out, ease-in-out, hold, or [x1,y1,x2,y2]. Paths animate too (same command count in every key). Tokens cannot be animated.

Keep it small: a few layers, round numbers, ≤ 2 s loops, start and end on the same pose for loops. Never use expressions.

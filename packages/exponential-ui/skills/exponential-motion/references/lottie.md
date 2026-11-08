# Writing Lottie directly

> Generated from packages/exponential-ui/eval/motion/lottie-guide.md (genUI 0.20.0) by packages/exponential-ui/scripts/generate-skills.ts — do not edit.

Lottie (Bodymovin) animation JSON. Root: {"v":"5.12.0","fr":60,"ip":0,"op":frames,"w":width,"h":height,"layers":[…],"assets":[]}. Times are FRAMES. Layers are listed TOP first.

Layer: {"ty":4 (shape) | 1 (solid: sc "#rrggbb", sw, sh) | 3 (null) | 0 (precomp: refId, w, h), "ind":n, "ip":0, "op":frames, "st":0, "ks":{transform}, "shapes":[…]}. Optional "parent": ind, "tt": 1 alpha matte (the layer above is the matte and needs "td":1), "masksProperties":[{"mode":"a","pt":{"a":0,"k":shape},"o":{"a":0,"k":100}}].

Every property is {"a":0,"k":value} or animated {"a":1,"k":[{"t":0,"s":[v],"o":{"x":[0.42],"y":[0]},"i":{"x":[0.58],"y":[1]}},{"t":36,"s":[v]}]}. Scalars are arrays in keyframes.

Transform "ks" / group "tr": "a" anchor [x,y], "p" position [x,y], "s" scale [100,100] (percent), "r" rotation (degrees), "o" opacity (0–100).

Shapes ("ty"), drawn top first; a style paints the shapes before it in the same group:
- "rc" rect {"p":centre,"s":size,"r":radius}, "el" ellipse {"p","s"}, "sr" star {"sy":1 star|2 polygon,"p","pt":points,"or","ir","os":0,"is":0,"r":0}, "sh" path {"ks":{"a":0,"k":{"c":true,"v":[[x,y]…],"i":[[dx,dy]…],"o":[[dx,dy]…]}}} (i/o tangents relative to each vertex);
- "fl" fill {"c":[r,g,b,1] (0–1),"o":100}, "st" stroke {"c","o","w","lc":2,"lj":2}, "gf" gradient fill {"t":1 linear|2 radial,"s":[x,y],"e":[x,y],"g":{"p":n,"k":{"a":0,"k":[offset,r,g,b,…]}},"o"};
- "tm" trim {"s":0,"e":100,"o":0,"m":1}, "rp" repeater {"c":copies,"o":0,"m":1,"tr":{"ty":"tr",…,"so":100,"eo":100}};
- "gr" group {"it":[…shapes, then the group transform {"ty":"tr","a","p","s","r","o"} LAST]}.

Keep it small: a few layers, ≤ 2 s loops, the last keyframe equal to the first for loops. Never use expressions.

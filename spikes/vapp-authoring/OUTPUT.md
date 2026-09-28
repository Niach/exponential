# VAPP-4 authoring probes: raw output

Run from `spikes/vapp-authoring/` with bun 1.3.14 on 2026-09-28. Installed: @stylexjs/stylex 0.19.1, @stylexjs/dev-runtime 0.11.1 (= npm latest), react-strict-dom 0.0.55 (pulls @stylexjs/babel-plugin + stylex 0.15.4, and bun auto-installed its react-native 0.87.1 peer), react/react-dom 19.3.0.

## 1. `bun probe-stylex-runtime.ts`

```
=== @stylexjs/stylex 0.19.1, plain runtime (no babel plugin)
--- exports
[
  "attrs",
  "create",
  "createTheme",
  "defaultMarker",
  "defineConsts",
  "defineMarker",
  "defineVars",
  "env",
  "firstThatWorks",
  "keyframes",
  "legacyMerge",
  "positionTry",
  "props",
  "types",
  "unstable_conditional",
  "unstable_createThemeNested",
  "unstable_defineConstsNested",
  "unstable_defineVarsNested",
  "viewTransitionClass",
  "when"
]
--- stylex.create(input)
THROWS: Unexpected 'stylex.create' call at runtime. Styles must be compiled by '@stylexjs/babel-plugin'.
--- stylex.props(styles.a)
{}
--- stylex.defineVars({card:'#111'})
THROWS: Unexpected 'stylex.defineVars' call at runtime. Styles must be compiled by '@stylexjs/babel-plugin'.

=== @stylexjs/dev-runtime (latest on npm) inject()
--- require('@stylexjs/dev-runtime')
THROWS: Cannot find module '@stylexjs/stylex/lib/StyleXSheet' from '/Users/niach/Exponential/repos/Niach/exponential.worktrees/exp-VAPP-4/spikes/vapp-authoring/node_modules/@stylexjs/dev-runtime/lib/index.js'

=== dev-runtime's create() alone (lib/stylex-create, bypassing the broken StyleXSheet import)
--- create(input)
{
  "a": {
    "display": "xrvj5dj",
    "gridTemplateAreas": "xfsrn5j",
    "width": "x3hqpx7",
    ":hover_opacity": "xj34u2y",
    "@media (min-width: 600px)_gap": "xf4sgm3",
    "@media (min-width: 600px)_rowGap": null,
    "@media (min-width: 600px)_columnGap": null,
    "$$css": true
  }
}
--- stylex.props(s.a) with the 0.19.1 runtime
{
  "className": "xrvj5dj xfsrn5j x3hqpx7 xj34u2y xf4sgm3"
}
--- inserted rules
[
  [
    "xrvj5dj",
    ".xrvj5dj{display:grid}",
    3000
  ],
  [
    "xfsrn5j",
    ".xfsrn5j{grid-template-areas:\"nav main\"}",
    2000
  ],
  [
    "x3hqpx7",
    ".x3hqpx7{width:50%}",
    4000
  ],
  [
    "xj34u2y",
    ".xj34u2y:hover{opacity:.5}",
    3130
  ],
  [
    "xf4sgm3",
    "@media (min-width: 600px){.xf4sgm3.xf4sgm3{gap:12px}}",
    2200
  ]
]
--- getStyleXCreate(...)
"ok"
```

## 2. `bun probe-rsd-native.ts` (react-native shimmed via `shims/register.ts`, preloaded by `bunfig.toml`)

```
loaded react-strict-dom native; css exports: __customProperties, create, createTheme, defineConsts, defineVars, firstThatWorks, keyframes, positionTry, props

=== A: the StyleX-subset shape our fixture uses (conditions as TOP-LEVEL keys)
[warn] React Strict DOM: unsupported style property "gridTemplateAreas"
[warn] React Strict DOM: unsupported style property "gridTemplateColumns"
--- css.create({a})
{
  "a": {
    "display": "grid",
    "width": "50%",
    "aspectRatio": "16/9",
    "flexBasis": "30%",
    "marginLeft": "auto",
    "padding": {
      "value": 1,
      "unit": "rem"
    },
    "borderRadius": {
      "value": 12,
      "unit": "px"
    },
    ":hover": {
      "opacity": 0.5
    },
    "@media (min-width: 600px)": {
      "gap": 12
    }
  }
}
[warn] React Strict DOM: "display:grid" is not a supported value
--- css.props(styles.a) with no options (this = undefined)
{
  "style": {
    "display": "grid",
    "width": "50%",
    "aspectRatio": "16/9",
    "flexBasis": "30%",
    "marginLeft": "auto",
    "padding": 16,
    "borderRadius": 12,
    ":hover": {
      "opacity": 0.5
    },
    "@media (min-width: 600px)": {
      "gap": 12
    }
  }
}
--- css.props.call({viewportWidth: 900}, styles.a)
{
  "style": {
    "display": "grid",
    "width": "50%",
    "aspectRatio": "16/9",
    "flexBasis": "30%",
    "marginLeft": "auto",
    "padding": 16,
    "borderRadius": 12,
    ":hover": {
      "opacity": 0.5
    },
    "@media (min-width: 600px)": {
      "gap": 12
    }
  }
}

=== B: StyleX's canonical shape (conditions INSIDE the value)
--- css.create({b})
{
  "a": {
    "display": "flex",
    "width": "50%",
    "padding": {
      "value": 1,
      "unit": "rem"
    },
    "opacity": {
      "default": 1,
      ":hover": 0.5
    },
    "gap": {
      "default": 4,
      "@media (min-width: 600px)": 12
    }
  }
}
--- css.props.call({"viewportWidth":390}, styles.a)
{
  "style": {
    "display": "flex",
    "width": "50%",
    "padding": 16,
    "opacity": 1,
    "gap": 4
  }
}
--- css.props.call({"viewportWidth":900,"hover":true}, styles.a)
{
  "style": {
    "display": "flex",
    "width": "50%",
    "padding": 16,
    "opacity": 0.5,
    "gap": 12
  }
}
--- css.props.call({"viewportWidth":900,"fontScale":1.5}, styles.a)
{
  "style": {
    "display": "flex",
    "width": "50%",
    "padding": 24,
    "opacity": 1,
    "gap": 12
  }
}

=== C: defineVars
--- css.defineVars({card})
{
  "card": "var(--card__id__1)"
}
--- create with a var
{
  "a": {
    "backgroundColor": {
      "_tokens": [
        {
          "_variable": "--card__id__1",
          "_fallback": null
        }
      ]
    }
  }
}
--- props with a var
{
  "style": {
    "backgroundColor": "#111"
  }
}
```

## 3. `bun probe-vapp-css.ts` + `bunx -p typescript@5.9 tsc -p .`

```
create(): validated 28 kitchen-sink styles, returned the same objects: true
props(s.base, s.grid, selected && s.dim) =
{
  "display": "grid",
  "gap": 8,
  "@media (min-width: 600px)": {
    "gap": 12,
    "gridTemplateColumns": "minmax(180px, 1fr) 2fr",
    "gridTemplateAreas": [
      "nav main",
      "nav footer"
    ]
  },
  ":pressed": {
    "opacity": 0.4
  },
  "gridTemplateColumns": "1fr",
  "gridTemplateAreas": [
    "nav",
    "main",
    "footer"
  ],
  "opacity": 0.8
}
JSON round-trip identical: true
rejects {"zIndex":3}: bad: "zIndex" is not in the vApp whitelist
rejects {":hover":{"opacity":0.5}}: bad: ":hover" is not in the vApp whitelist
rejects {"@media (min-width: 600px)":{":pressed":{"opacity":1}}}: bad.@media (min-width: 600px): conditions do not nest (:pressed)
tsc: typed keys OK (3 expected errors fired: zIndex, display inline-grid, backgroundColor red)
```

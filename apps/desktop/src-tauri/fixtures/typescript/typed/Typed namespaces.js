// TypeScript 5.9.3 (Monaco 0.57.0), transpileModule for ES2020, before #381. Source SHA-256: 0963ca0805f948f8af4ea27a587e7eef4dea3179006d7dd3daf5afe070ce13b7
// Declarations TypeScript has and JavaScript does not: namespaces, string
// enums, ambient declarations, type-level code that leaves nothing behind.
//
// Probes: a namespace with a nested one, a string enum, `declare`, function
// overloads, `keyof typeof`, mapped and conditional types, an assertion
// function, `export type` and a type-only re-export, `import type` of a
// namespace.
import { defineEffect, hsv, lerp } from '@candeo/effects-api';
var Mode;
(function (Mode) {
    Mode["Wave"] = "wave";
    Mode["Pulse"] = "pulse";
})(Mode || (Mode = {}));
var Palette;
(function (Palette) {
    Palette.BASE = 200;
    function at(i) {
        return (Palette.BASE + i * Inner.STEP) % 360;
    }
    Palette.at = at;
    let Inner;
    (function (Inner) {
        Inner.STEP = 23;
    })(Inner = Palette.Inner || (Palette.Inner = {}));
})(Palette || (Palette = {}));
const SPEEDS = { slow: 0.5, normal: 1, fast: 2 };
function assertKnown(mode) {
    if (mode !== Mode.Wave && mode !== Mode.Pulse)
        throw new Error(`unknown mode ${mode}`);
}
function level(t, speed = 'normal') {
    const levels = SPEEDS;
    return (Math.sin(t * levels[speed] * Math.PI) + 1) / 2;
}
export default defineEffect({
    description: 'Hues from a namespace, by mode',
    params: {
        mode: { kind: 'choice', label: 'Mode', options: ['wave', 'pulse'], default: 'wave' },
    },
    render({ layout, time, frame, params }) {
        const mode = params.mode;
        assertKnown(mode);
        layout.keys.forEach((key, i) => {
            const v = mode === Mode.Pulse ? level(time, 'fast') : lerp(0.2, 1, level(time + i / 10));
            frame.set(key, hsv(Palette.at(i), 1, v));
        });
    },
});

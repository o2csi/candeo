// TypeScript 5.9.3 (Monaco 0.57.0), transpileModule for ES2020, before #381. Source SHA-256: 629081fd6b41091d19b4c865d81e3bf0de25e00b30668e4437695eb7622af230
// Particles kept from frame to frame in classes: the syntax ES2020 does not
// have and TypeScript lowers for it, and the TypeScript-only class features.
//
// Probes: class fields with and without initialisers, `#private` fields and
// methods, static fields and a static block, parameter properties, `readonly`,
// `abstract`, `override`, accessors, method overloads, an enum, a const enum,
// logical assignment, numeric separators, optional chaining, `#x in obj`.
var __classPrivateFieldGet = (this && this.__classPrivateFieldGet) || function (receiver, state, kind, f) {
    if (kind === "a" && !f) throw new TypeError("Private accessor was defined without a getter");
    if (typeof state === "function" ? receiver !== state || !f : !state.has(receiver)) throw new TypeError("Cannot read private member from an object whose class did not declare it");
    return kind === "m" ? f : kind === "a" ? f.call(receiver) : f ? f.value : state.get(receiver);
};
var __classPrivateFieldSet = (this && this.__classPrivateFieldSet) || function (receiver, state, value, kind, f) {
    if (kind === "m") throw new TypeError("Private method is not writable");
    if (kind === "a" && !f) throw new TypeError("Private accessor was defined without a setter");
    if (typeof state === "function" ? receiver !== state || !f : !state.has(receiver)) throw new TypeError("Cannot write private member to an object whose class did not declare it");
    return (kind === "a" ? f.call(receiver, value) : f ? f.value = value : state.set(receiver, value)), value;
};
var __classPrivateFieldIn = (this && this.__classPrivateFieldIn) || function(state, receiver) {
    if (receiver === null || (typeof receiver !== "object" && typeof receiver !== "function")) throw new TypeError("Cannot use 'in' operator on non-object");
    return typeof state === "function" ? receiver === state : state.has(receiver);
};
var _Body_age, _Spark_fade;
import { defineEffect, hsv, mix } from '@candeo/effects-api';
var Kind;
(function (Kind) {
    Kind[Kind["Spark"] = 0] = "Spark";
    Kind[Kind["Ember"] = 4] = "Ember";
    Kind[Kind["Smoke"] = 5] = "Smoke";
})(Kind || (Kind = {}));
var Fade;
(function (Fade) {
    Fade[Fade["Fast"] = 0.5] = "Fast";
    Fade[Fade["Slow"] = 1.5] = "Slow";
})(Fade || (Fade = {}));
class Body {
    constructor(origin, kind) {
        this.origin = origin;
        this.kind = kind;
        _Body_age.set(this, 0);
        this.hue = (origin * 37) % 360;
        Body.created++;
    }
    get age() {
        return __classPrivateFieldGet(this, _Body_age, "f");
    }
    step(dt) {
        __classPrivateFieldSet(this, _Body_age, __classPrivateFieldGet(this, _Body_age, "f") + dt, "f");
        return __classPrivateFieldGet(this, _Body_age, "f") < this.life();
    }
    static isBody(x) {
        return typeof x === 'object' && x !== null && __classPrivateFieldIn(_Body_age, x);
    }
}
_Body_age = new WeakMap();
Body.created = 0;
Body.LIMIT = 1000;
(() => {
    Body.created = 0;
})();
class Spark extends Body {
    constructor() {
        super(...arguments);
        _Spark_fade.set(this, Fade.Fast);
    }
    life() {
        return this.kind === Kind.Ember ? __classPrivateFieldGet(this, _Spark_fade, "f") * 2 : __classPrivateFieldGet(this, _Spark_fade, "f");
    }
    shade(t, base) {
        this.colour ?? (this.colour = hsv(this.hue, 1, 1));
        return mix(base ?? this.colour, { r: 0, g: 0, b: 0 }, Math.min(1, t));
    }
}
_Spark_fade = new WeakMap();
const live = [];
let last = 0;
export default defineEffect({
    description: 'Sparks that fade, kept in classes',
    params: {
        rate: { kind: 'number', label: 'Rate', min: 1, max: 20, default: 6 },
    },
    render({ layout, time, frameIndex, frame, params }) {
        const dt = Math.max(0, time - last);
        last = time;
        const keys = layout.keys;
        if (keys.length === 0)
            return;
        if (frameIndex % Math.max(1, Math.round(30 / params.rate)) === 0) {
            const at = (frameIndex * 7919) % keys.length;
            live.push(new Spark(at, frameIndex % 3 === 0 ? Kind.Ember : Kind.Spark));
        }
        for (let i = live.length - 1; i >= 0; i--) {
            const spark = live[i];
            if (!spark.step(dt) || !Body.isBody(spark))
                live.splice(i, 1);
        }
        let lit = 0;
        for (const spark of live) {
            const key = keys[spark.origin];
            if (key?.index === undefined)
                continue;
            frame.set(key, spark.shade(spark.age / spark.life()));
            lit || (lit = 1);
            lit && (lit = lit + 1);
        }
        if (Body.created > Body.LIMIT)
            live.length = 0;
    },
});

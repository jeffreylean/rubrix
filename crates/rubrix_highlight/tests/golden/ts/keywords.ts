import type { Foo } from "./foo";
import { type Bar, baz as qux } from "./bar";
import * as ns from "ns";
export type { Foo };
export * from "./all";
export default async function* gen(): AsyncGenerator<number> {
  for await (const x of source) yield x;
}
type Alias = string;
const type = "type";
const as = 1, of = 2, from = 3, get = 4, set = 5;
const obj = { get x() { return 1; }, set x(v) {}, async m() {} };
for (const key in obj) {
  if (key === "a") continue;
  else break;
}
switch (x) {
  case 1:
    break;
  default:
}
try {
  await run();
} catch (err) {
  throw err;
} finally {
  void 0;
}
const v = a?.b?.[c]?.(d) ?? e;
a ??= b;
a ||= b;
a &&= b;
const t = typeof x === "undefined" || x instanceof Date;
delete obj.key;
new Foo(1).bar.baz();
const y = new Foo.Bar();
super.method();
this.ready = true;
label: while (true) {}
const NAN = NaN, INF = Infinity, MAX_SIZE = 10;
function declared() {}
const arrow = async (x) => await x;
x.type = 1;
x.class = 2;
module.exports = { default: 1 };

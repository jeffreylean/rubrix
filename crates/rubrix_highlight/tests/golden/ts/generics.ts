function identity<T>(value: T): T {
  return value;
}
class Box<T extends object = {}> extends Base<T> implements Item<T> {}
interface Pair<K, V> {
  key: K;
  value: V;
}
type Mapped<T> = { readonly [K in keyof T]?: T[K] };
type Cond<T> = T extends string ? "s" : T extends number ? "n" : never;
const map = new Map<string, Array<number>>();
const fn = <T,>(x: T) => x;
let bits = a >> 2 > b;
const result = useState<string | null>(null);
function assertIsString(val: any): asserts val is string {}
declare module "x" {
  export type Opt = Partial<Record<string, unknown>>;
}
enum Color {
  Red = 1,
  Green,
}
abstract class Shape {
  abstract area(): number;
  protected readonly id: bigint = 0n;
}
let u: undefined | null | void | symbol | object | boolean;
let v = obj satisfies Shape as unknown as Circle;
namespace Geometry.Shapes {}

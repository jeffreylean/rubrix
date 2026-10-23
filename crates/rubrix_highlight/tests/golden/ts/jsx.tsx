const el = <div className="a">{x}</div>;
const self = <Input value={value} onChange={() => set(1)} />;
const nested = <A><B>{`t ${v}`}</B></A>;
function App(): JSX.Element {
  return <ul>{items.map((i) => <li key={i}>{i}</li>)}</ul>;
}

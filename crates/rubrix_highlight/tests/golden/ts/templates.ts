const s = `a ${`b ${c}`} d`;
const deep = `1 ${`2 ${`3 ${`4 ${x}`}`}`} end`;
const multi = `first line
  second ${value} line
  ${a + b}
  third line`;
const obj = `${{ a: 1 }.a} and ${[1, 2].map((n) => `<${n}>`).join("")}`;
const nested = `outer ${
  inner(`
    deeper ${a}
    still deeper
  `)
} tail`;
const escaped = `\${not code} \` \u{1F600} \x41 \n ${ok}`;
const dollar = `$ ${x} $y $`;
const tagged = html`<p>${name}</p>`;
const empty = ``;
const arrow = `${() => { return { b: 1 }; }}`;

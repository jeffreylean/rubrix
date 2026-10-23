"use strict";
const fs = require("fs");
function legacy(a, b) {
  var self = this;
  return a === b ? /x/.test(a) : a / b;
}
module.exports = { legacy, count: 0x10, big: 10n, name: `v${1}` };
exports.run = async () => {
  for (const [k, v] of Object.entries(module.exports)) {
    console.log(`${k}=${v}`);
  }
};

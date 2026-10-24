class Counter {
  #count = 0;
  static #instances = 0;
  #bump(): void {
    this.#count++;
  }
  has(other: object): boolean {
    return #count in other;
  }
  get value() {
    return this.#count;
  }
}

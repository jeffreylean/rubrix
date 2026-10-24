@Component({ selector: "app" })
class A {
  @Input() name: string;
  @Output() changed = new EventEmitter<string>();
  @HostListener("window:resize", ["$event"])
  onResize(event: Event) {}
  @a.b.c() deep = 1;
  @Injectable
  static create() {}
}
export default @dec class B {}

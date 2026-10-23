/* a /* b */ still */ fn x() {}
/* outer
   /* inner
   */ still outer
*/ let after = 1;
/**/ let empty = 2;
/*** not doc */
let x = 1 /* tail */ + 2;
/* unclosed /* deep

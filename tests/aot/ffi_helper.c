/* Built by scripts/aot_parity.sh and referenced from 26_ffi.zera as @FFI_LIB@.
   Zera's C-FFI marshals every numeric as a double, so that is the ABI here. */
double zera_add_d(double a, double b) { return a + b; }
double zera_square(double a) { return a * a; }

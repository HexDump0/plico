`srgb.icc` and `ps_cmyk.icc` are Ghostscript 10.08's `iccprofiles/` files,
copyright Artifex Software, AGPL-3.0-or-later. `srgb.icc` is the PDF/A output
intent. `ps_cmyk.icc` maps CMYK the way viewers do for uncalibrated DeviceCMYK,
so declaring it as DefaultCMYK leaves CMYK content looking as it did.

"""Cuts the three static Manrope weights the app uses out of the variable font (assets/fonts/Manrope.ttf).
Static instances carry correct advance widths and kerning for every weight; egui's variable-font coordinates only changed
the outlines, which made bold Cyrillic look unevenly spaced.
usage: python make_fonts.py"""
import pathlib
from fontTools.ttLib import TTFont
from fontTools.varLib.instancer import instantiateVariableFont

fonts = pathlib.Path(__file__).resolve().parents[1] / "assets" / "fonts"
for name, wght in [("Regular", 450), ("Light", 300), ("Bold", 700)]:
    f = TTFont(fonts / "Manrope.ttf")
    inst = instantiateVariableFont(f, {"wght": wght}, inplace=False)
    out = fonts / f"Manrope-{name}.ttf"
    inst.save(out)
    print(out.name, out.stat().st_size)

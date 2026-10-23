/* Independent FreeType check of the optional horizontal format-0 kern table. */
#include <ft2build.h>
#include FT_FREETYPE_H
#include <stdio.h>
int main(int argc, char **argv) {
    FT_Library library;
    FT_Face face;
    FT_Vector delta;
    if (argc != 2 || FT_Init_FreeType(&library) || FT_New_Face(library, argv[1], 0, &face)) return 1;
    if (!FT_HAS_KERNING(face) || FT_Get_Kerning(face, FT_Get_Char_Index(face, 'A'), FT_Get_Char_Index(face, 'V'), FT_KERNING_UNSCALED, &delta) || delta.x != -128) return 2;
    FT_Done_Face(face);
    FT_Done_FreeType(library);
    puts("FreeType reads the AV kerning pair: -128 font units");
    return 0;
}

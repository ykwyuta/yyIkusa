"""Pack existing yyMongolia BGRA cells without external Python dependencies.

This copies static yyMongolia props and appends the dedicated directional walk
cells prepared by pack_unit_motion.ps1. Hawks and palisades use native geometry.
"""
from pathlib import Path
import struct
R=Path(__file__).resolve().parents[1]
SIZE=128
def cell(name,index):
    raw=(R/'assets/mongolia'/f'{name}.bgra').read_bytes()
    raw=raw[index*192*192*4:(index+1)*192*192*4]
    assert len(raw)==192*192*4
    out=bytearray()
    for y in range(SIZE):
        for x in range(SIZE):
            i=((y*192//SIZE)*192+x*192//SIZE)*4
            b,g,r,a=raw[i:i+4]
            # Sprite shaders use straight alpha (source files are premultiplied BGRA).
            out.extend((min(255,r*255//a) if a else 0,
                        min(255,g*255//a) if a else 0,
                        min(255,b*255//a) if a else 0,a))
    return out
def encode(raw,w,h):
    px=[raw[i:i+4] for i in range(0,len(raw),4)]
    out=bytearray(b'YYSP'+struct.pack('<HH',w,h)); i=0
    while i<len(px):
        run=1
        while i+run<len(px) and run<129 and px[i+run]==px[i]: run+=1
        if run>=2:
            out.append(126+run); out.extend(px[i]); i+=run
        else:
            start=i; i+=1
            while i<len(px) and i-start<128:
                if i+1<len(px) and px[i]==px[i+1]: break
                i+=1
            out.append(i-start-1)
            for p in px[start:i]: out.extend(p)
    return out
blank=bytes(SIZE*SIZE*4)
tiles=[cell('southern',5),cell('steppe',1),cell('japan',1),blank,
       cell('steppe',4),cell('steppe',3),cell('steppe',3),cell('steppe',2),
       cell('steppe',1),blank,blank,blank,cell('japan',6)]
tiles.extend([blank]*(16-len(tiles)))
motion=(R/'assets/units/walk_cells.rgba').read_bytes()
assert len(motion)==8*25*SIZE*SIZE*4, 'Expected 8 types, 5 directions, 5 frames'
tiles.extend(motion[i:i+SIZE*SIZE*4] for i in range(0,len(motion),SIZE*SIZE*4))
COLS,ROWS=16,14
atlas=bytearray(COLS*SIZE*ROWS*SIZE*4)
for n,tile in enumerate(tiles):
    for y in range(SIZE):
        off=((n//COLS*SIZE+y)*COLS*SIZE+n%COLS*SIZE)*4
        atlas[off:off+SIZE*4]=tile[y*SIZE*4:(y+1)*SIZE*4]
(R/'assets/sprites/oblique_atlas.rle').write_bytes(encode(atlas,COLS*SIZE,ROWS*SIZE))
# Dedicated title artwork is packed separately by tools/pack_brand.ps1.
print('Packed 200 movement cells, static props and period headquarters.')

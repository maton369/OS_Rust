curl -s https://hikalium.com/bmp/white_diamond_5x5.bmp.hex > bmp.hex
cat bmp.hex | sed 's/ffffff/ff0000/g' | sed 's/000000/00ffff/g' > bmp2.hex
cat bmp2.hex | xxd -r -p > bmp2.bmp
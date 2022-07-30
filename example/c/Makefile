build: *.c
	$(CC) -fuse-ld=$(shell realpath ../../linker.sh) -o example-machop main.c

build_ld: *.c
	$(CC) -o example-ld main.c

build_mold: *.c
	$(CC) -fuse-ld=mold -o example-mold main.c

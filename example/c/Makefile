build: *.c
	gcc -fuse-ld=$(shell realpath ../../linker.sh) -o example-machop main.c

build_ld: *.c
	$(CC) -o example-ld main.c

build_mold: *.c
	$(CC) -fuse-ld=mold -o example-mold main.c

build_no_link: *c
	gcc -c -o example.o main.c

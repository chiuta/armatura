#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "../armatura.h"

int main(int argc, char **argv) {
    if (argc < 2) return 2;
    FILE *f = fopen(argv[1], "rb");
    if (!f) return 3;
    fseek(f, 0, SEEK_END); long n = ftell(f); fseek(f, 0, SEEK_SET);
    char *json = malloc(n);
    if (fread(json, 1, n, f) != (size_t)n) return 4;
    fclose(f);
    ArmaturaEngine *e = armatura_open(json, n);
    if (!e) { printf("open failed: %s\n", armatura_last_error()); return 5; }
    uint32_t w, h;
    armatura_render_rgba(e, 0, 0.5f, NULL, 0, &w, &h);
    uint8_t *px = malloc((size_t)w * h * 4);
    int rc = armatura_render_rgba(e, 10, 0.5f, px, (size_t)w * h * 4, &w, &h);
    long sum = 0; for (size_t i = 0; i < (size_t)w * h * 4; i++) sum += px[i];
    printf("abi %u, %u frames, %ux%u, rc=%d, checksum=%ld\n", armatura_abi_version(), armatura_frame_count(e), w, h, rc, sum);
    armatura_close(e); free(px); free(json);
    return rc == 0 && sum > 0 ? 0 : 1;
}

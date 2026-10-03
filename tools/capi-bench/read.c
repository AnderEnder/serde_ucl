/* Spec-owned read benchmark; same released-interface caller for all libraries. */
#define _POSIX_C_SOURCE 200809L
#include "ucl.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>

static volatile unsigned int sink;
static void fail(const char *message) { fprintf(stderr, "%s\n", message); exit(1); }
static double now(void) {
    struct timespec value;
    if (clock_gettime(CLOCK_MONOTONIC, &value)) fail("clock failed");
    return value.tv_sec + value.tv_nsec * 1e-9;
}
static unsigned int visit(const ucl_object_t *target, int first) {
    ucl_object_iter_t iter = ucl_object_iterate_new(target);
    if (!iter) fail("iterator allocation failed");
    unsigned int count = 0;
    const ucl_object_t *child;
    while ((child = ucl_object_iterate_safe(iter, true))) {
        sink += child->len;
        count++;
        if (first) break;
    }
    ucl_object_iterate_free(iter);
    return count;
}
int main(int argc, char **argv) {
    if (argc != 5) fail("usage: read FILE KEY|- first|full|lookup ITERATIONS");
    FILE *file = fopen(argv[1], "rb");
    if (!file || fseek(file, 0, SEEK_END)) fail("input open/seek failed");
    long size = ftell(file);
    if (size < 0 || fseek(file, 0, SEEK_SET)) fail("input size failed");
    unsigned char *data = malloc((size_t)size + 1);
    if (!data || fread(data, 1, (size_t)size, file) != (size_t)size) fail("input read failed");
    fclose(file);
    struct ucl_parser *parser = ucl_parser_new(0);
    if (!parser || !ucl_parser_add_chunk(parser, data, (size_t)size)) fail("parse failed");
    ucl_object_t *root = ucl_parser_get_object(parser);
    ucl_parser_free(parser);
    if (!root) fail("no root");
    const ucl_object_t *target = strcmp(argv[2], "-") ? ucl_object_lookup(root, argv[2]) : root;
    if (!target) fail("no target");
    int first = !strcmp(argv[3], "first"), full = !strcmp(argv[3], "full");
    int lookup = !strcmp(argv[3], "lookup");
    if (!first && !full && !lookup) fail("invalid mode");
    char *end;
    unsigned long iterations = strtoul(argv[4], &end, 10);
    if (!iterations || *end) fail("invalid iteration count");
    unsigned int count = lookup ? 1 : visit(target, first);
    if (!count) fail("empty traversal");
    double start = now();
    for (unsigned long i = 0; i < iterations; i++) {
        if (lookup) {
            const ucl_object_t *value = ucl_object_lookup(root, argv[2]);
            if (!value) fail("lookup failed");
            sink += value->len;
        } else if (visit(target, first) != count) fail("inconsistent traversal");
    }
    double elapsed = now() - start;
    printf("{\"seconds\":%.12f,\"count\":%u}\n", elapsed, count);
    ucl_object_unref(root);
    free(data);
    return 0;
}

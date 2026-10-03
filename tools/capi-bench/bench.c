/* Spec-owned benchmark: identical public C caller for both libraries. */
#define _POSIX_C_SOURCE 200809L
#include "ucl.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>

static volatile unsigned int sink;

static void fail(const char *message) {
    fprintf(stderr, "%s\n", message);
    exit(1);
}

static double now(void) {
    struct timespec value;
    if (clock_gettime(CLOCK_MONOTONIC, &value) != 0) fail("clock failed");
    return (double)value.tv_sec + (double)value.tv_nsec * 1e-9;
}

static ucl_object_t *parse(const unsigned char *data, size_t length) {
    struct ucl_parser *parser = ucl_parser_new(0);
    if (!parser) fail("parser allocation failed");
    if (!ucl_parser_add_chunk(parser, data, length)) {
        const char *error = ucl_parser_get_error(parser);
        fail(error ? error : "parse failed");
    }
    ucl_object_t *object = ucl_parser_get_object(parser);
    ucl_parser_free(parser);
    if (!object) fail("no parsed object");
    return object;
}

int main(int argc, char **argv) {
    if (argc != 4) fail("usage: bench FILE parse|emit|dump|phases ITERATIONS");
    FILE *file = fopen(argv[1], "rb");
    if (!file) fail("cannot open input");
    if (fseek(file, 0, SEEK_END) != 0) fail("cannot seek input");
    long size = ftell(file);
    if (size < 0 || fseek(file, 0, SEEK_SET) != 0) fail("cannot size input");
    unsigned char *data = malloc((size_t)size + 1);
    if (!data) fail("input allocation failed");
    if (fread(data, 1, (size_t)size, file) != (size_t)size) fail("cannot read input");
    fclose(file);
    data[size] = 0;
    char *end = NULL;
    unsigned long iterations = strtoul(argv[3], &end, 10);
    if (!iterations || !end || *end) fail("invalid iterations");
    int parsing = strcmp(argv[2], "parse") == 0;
    int emitting = strcmp(argv[2], "emit") == 0;
    int dumping = strcmp(argv[2], "dump") == 0;
    int phases = strcmp(argv[2], "phases") == 0;
    if (!parsing && !emitting && !dumping && !phases) fail("invalid mode");
    ucl_object_t *object = (parsing || phases) ? NULL : parse(data, (size_t)size);
    if (phases) {
        double sums[5] = {0};
        for (unsigned long i = 0; i < iterations; i++) {
            double t0 = now();
            struct ucl_parser *parser = ucl_parser_new(0);
            double t1 = now();
            if (!parser) fail("parser allocation failed");
            if (!ucl_parser_add_chunk(parser, data, (size_t)size)) fail("parse failed");
            double t2 = now();
            ucl_object_t *value = ucl_parser_get_object(parser);
            double t3 = now();
            if (!value) fail("no parsed object");
            ucl_parser_free(parser);
            double t4 = now();
            sink += value->len;
            ucl_object_unref(value);
            double t5 = now();
            sums[0] += t1-t0; sums[1] += t2-t1; sums[2] += t3-t2;
            sums[3] += t4-t3; sums[4] += t5-t4;
        }
        printf("{\"parser_new\":%.12f,\"add_chunk\":%.12f,"
               "\"get_object\":%.12f,\"parser_free\":%.12f,\"object_unref\":%.12f}\n",
               sums[0]/iterations, sums[1]/iterations, sums[2]/iterations,
               sums[3]/iterations, sums[4]/iterations);
    } else if (dumping) {
        unsigned char *output = ucl_object_emit(object, UCL_EMIT_JSON_COMPACT);
        if (!output) fail("emit failed");
        if (fwrite(output, 1, strlen((const char *)output), stdout) == 0)
            fail("empty output");
        free(output);
    } else {
        double start = now();
        for (unsigned long i = 0; i < iterations; i++) {
            if (parsing) {
                ucl_object_t *value = parse(data, (size_t)size);
                sink += value->len;
                ucl_object_unref(value);
            } else {
                unsigned char *output = ucl_object_emit(object, UCL_EMIT_JSON_COMPACT);
                if (!output) fail("emit failed");
                sink += output[0];
                free(output);
            }
        }
        double elapsed = now() - start;
        printf("%.12f\n", elapsed);
    }
    if (object) ucl_object_unref(object);
    free(data);
    return 0;
}

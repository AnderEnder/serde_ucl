/*
 * Times libucl on documents, for the README's comparison (scripts/bench-compare.sh).
 *
 *   lucl [-v NAME=VALUE]... FILE...
 *
 * For each file, parses its bytes as a chunk with a new parser (flags 0), takes the object and
 * frees both, and prints "libucl|parse|<file name>|<seconds>": the median over 31 samples, each
 * at least about 5 ms of repetitions. "parse-nofree" is the same with the objects kept and
 * freed after each sample's timing. -v registers a variable on every parser, for documents that
 * include files through one (benches/corpus/README.md).
 */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#include "ucl.h"

#define SAMPLES 31
#define MAX_VARS 16

static const char *var_names[MAX_VARS];
static const char *var_values[MAX_VARS];
static int nvars;

static double now(void)
{
	struct timespec t;
	clock_gettime(CLOCK_MONOTONIC, &t);
	return t.tv_sec + t.tv_nsec / 1e9;
}

static int cmp(const void *a, const void *b)
{
	double x = *(const double *)a, y = *(const double *)b;
	return x < y ? -1 : x > y;
}

static ucl_object_t *parse(const unsigned char *buf, size_t n)
{
	struct ucl_parser *p = ucl_parser_new(0);
	for (int i = 0; i < nvars; i++)
		ucl_parser_register_variable(p, var_names[i], var_values[i]);
	if (!ucl_parser_add_chunk(p, buf, n)) {
		fprintf(stderr, "%s\n", ucl_parser_get_error(p));
		exit(1);
	}
	ucl_object_t *o = ucl_parser_get_object(p);
	ucl_parser_free(p);
	return o;
}

int main(int argc, char **argv)
{
	int f = 1;
	for (; f + 1 < argc && strcmp(argv[f], "-v") == 0; f += 2) {
		char *eq = strchr(argv[f + 1], '=');
		if (eq == NULL || nvars == MAX_VARS) {
			fprintf(stderr, "usage: lucl [-v NAME=VALUE]... FILE...\n");
			return 2;
		}
		*eq = '\0';
		var_names[nvars] = argv[f + 1];
		var_values[nvars] = eq + 1;
		nvars++;
	}
	for (; f < argc; f++) {
		FILE *fp = fopen(argv[f], "rb");
		if (fp == NULL) {
			perror(argv[f]);
			return 1;
		}
		fseek(fp, 0, SEEK_END);
		long n = ftell(fp);
		rewind(fp);
		unsigned char *buf = malloc(n > 0 ? n : 1);
		if (fread(buf, 1, n, fp) != (size_t)n) {
			perror(argv[f]);
			return 1;
		}
		fclose(fp);
		const char *name = strrchr(argv[f], '/') ? strrchr(argv[f], '/') + 1 : argv[f];

		double t0 = now();
		ucl_object_unref(parse(buf, n));
		double one = now() - t0;
		long reps = one > 0 ? (long)(0.005 / one) : 1000;
		if (reps < 1)
			reps = 1;
		double s[SAMPLES];

		for (int i = 0; i < SAMPLES; i++) {
			t0 = now();
			for (long r = 0; r < reps; r++)
				ucl_object_unref(parse(buf, n));
			s[i] = (now() - t0) / reps;
		}
		qsort(s, SAMPLES, sizeof(double), cmp);
		printf("libucl|parse|%s|%.9f\n", name, s[SAMPLES / 2]);

		ucl_object_t **keep = malloc(sizeof(*keep) * reps);
		for (int i = 0; i < SAMPLES; i++) {
			t0 = now();
			for (long r = 0; r < reps; r++)
				keep[r] = parse(buf, n);
			s[i] = (now() - t0) / reps;
			for (long r = 0; r < reps; r++)
				ucl_object_unref(keep[r]);
		}
		qsort(s, SAMPLES, sizeof(double), cmp);
		printf("libucl|parse-nofree|%s|%.9f\n", name, s[SAMPLES / 2]);
		fflush(stdout);
		free(keep);
		free(buf);
	}
	return 0;
}

/*
 * ucl-dump: typed canonical dump of a libucl parse result.
 *
 * This is the conformance oracle for ucl-rust-lexer (PLAN.md P0.2). It links
 * libucl (BSD-2-Clause, see LICENSE-libucl) and prints one JSON document to
 * stdout describing exactly what libucl built:
 *
 *   node   := {"t":"object","entries":[entry,...]}
 *           | {"t":"array","v":[node,...]}
 *           | {"t":"int","v":"<decimal i64>"}
 *           | {"t":"float","v":"<%.17g>"}
 *           | {"t":"time","v":"<%.17g seconds>"}
 *           | {"t":"string","v":"<utf-8>"}   or  {"t":"string","hex":"<bytes>"}
 *           | {"t":"bool","v":true|false}
 *           | {"t":"null"} | {"t":"userdata"}
 *   entry  := {"k":"<utf-8 key>","v":[node,...]}   or "khex" for a non-UTF-8 key.
 *             "v" has more than one element for an implicit array
 *             (UCL_OBJECT_MULTIVALUE); an explicit array is a single node of
 *             type "array".
 *   Any node whose libucl priority is non-zero carries "pri":<n>.
 *
 * A parse failure prints {"error":true}. The message is deliberately left out:
 * it contains absolute paths, and the conformance runner compares only whether
 * both implementations fail.
 *
 * Parser setup mirrors libucl's tests/test_basic.c, except that key
 * lowercasing is off by default (the crate's default must match libucl's
 * default, UCL_PARSER_DEFAULT):
 *   - variable ABI is registered as "unknown";
 *   - file variables (FILENAME, CURDIR) are set from the input path with
 *     realpath expansion, unless -F is given;
 *   - the input is added as one chunk, priority 0, UCL_DUPLICATE_APPEND.
 *
 * Usage: ucl-dump [-l] [-z] [-T] [-I] [-C] [-M] [-F] <file>
 *   -l  UCL_PARSER_KEY_LOWERCASE      -z  UCL_PARSER_ZEROCOPY
 *   -T  UCL_PARSER_NO_TIME            -I  UCL_PARSER_NO_IMPLICIT_ARRAYS
 *   -C  UCL_PARSER_SAVE_COMMENTS      -M  UCL_PARSER_DISABLE_MACRO
 *   -F  UCL_PARSER_NO_FILEVARS (and do not set file variables)
 */
#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

#include "ucl.h"

static bool
valid_utf8(const unsigned char *s, size_t len)
{
	size_t i = 0;

	while (i < len) {
		unsigned char c = s[i];
		size_t n;
		uint32_t cp;

		if (c < 0x80) {
			i++;
			continue;
		}
		else if ((c & 0xE0) == 0xC0) {
			n = 1;
			cp = c & 0x1F;
		}
		else if ((c & 0xF0) == 0xE0) {
			n = 2;
			cp = c & 0x0F;
		}
		else if ((c & 0xF8) == 0xF0) {
			n = 3;
			cp = c & 0x07;
		}
		else {
			return false;
		}
		if (i + n >= len) {
			/* Truncated sequence */
			return false;
		}
		for (size_t j = 1; j <= n; j++) {
			if ((s[i + j] & 0xC0) != 0x80) {
				return false;
			}
			cp = (cp << 6) | (s[i + j] & 0x3F);
		}
		/* Reject overlong forms, surrogates and out-of-range code points */
		if ((n == 1 && cp < 0x80) || (n == 2 && cp < 0x800) ||
			(n == 3 && cp < 0x10000) || cp > 0x10FFFF ||
			(cp >= 0xD800 && cp <= 0xDFFF)) {
			return false;
		}
		i += n + 1;
	}

	return true;
}

static void
put_json_string(const unsigned char *s, size_t len)
{
	putchar('"');
	for (size_t i = 0; i < len; i++) {
		unsigned char c = s[i];

		switch (c) {
		case '"':
			fputs("\\\"", stdout);
			break;
		case '\\':
			fputs("\\\\", stdout);
			break;
		case '\n':
			fputs("\\n", stdout);
			break;
		case '\r':
			fputs("\\r", stdout);
			break;
		case '\t':
			fputs("\\t", stdout);
			break;
		default:
			if (c < 0x20 || c == 0x7F) {
				printf("\\u%04x", c);
			}
			else {
				putchar(c);
			}
			break;
		}
	}
	putchar('"');
}

static void
put_hex(const unsigned char *s, size_t len)
{
	putchar('"');
	for (size_t i = 0; i < len; i++) {
		printf("%02x", s[i]);
	}
	putchar('"');
}

/* Writes "<name>":"..." for a byte string, choosing the hex form when needed. */
static void
put_bytes_field(const char *name, const char *hexname, const unsigned char *s, size_t len)
{
	if (valid_utf8(s, len)) {
		printf("\"%s\":", name);
		put_json_string(s, len);
	}
	else {
		printf("\"%s\":", hexname);
		put_hex(s, len);
	}
}

static void dump_node(const ucl_object_t *obj);

static void
dump_object(const ucl_object_t *obj)
{
	ucl_object_iter_t it = NULL;
	const ucl_object_t *head;
	bool first = true;

	fputs("\"t\":\"object\",\"entries\":[", stdout);
	/* expand_values = true walks the hash in insertion order, one head per key */
	while ((head = ucl_object_iterate(obj, &it, true)) != NULL) {
		const ucl_object_t *cur;
		bool first_value = true;

		if (!first) {
			putchar(',');
		}
		first = false;
		putchar('{');
		put_bytes_field("k", "khex", (const unsigned char *) head->key, head->keylen);
		fputs(",\"v\":[", stdout);
		/* Implicit arrays are a sibling chain through ->next */
		for (cur = head; cur != NULL; cur = cur->next) {
			if (!first_value) {
				putchar(',');
			}
			first_value = false;
			dump_node(cur);
		}
		fputs("]}", stdout);
	}
	putchar(']');
}

static void
dump_array(const ucl_object_t *obj)
{
	ucl_object_iter_t it = NULL;
	const ucl_object_t *elt;
	bool first = true;

	fputs("\"t\":\"array\",\"v\":[", stdout);
	while ((elt = ucl_object_iterate(obj, &it, true)) != NULL) {
		if (!first) {
			putchar(',');
		}
		first = false;
		dump_node(elt);
	}
	putchar(']');
}

static void
dump_node(const ucl_object_t *obj)
{
	size_t len = 0;
	const char *s;
	unsigned int pri;

	putchar('{');
	switch (obj->type) {
	case UCL_OBJECT:
		dump_object(obj);
		break;
	case UCL_ARRAY:
		dump_array(obj);
		break;
	case UCL_INT:
		printf("\"t\":\"int\",\"v\":\"%lld\"", (long long) ucl_object_toint(obj));
		break;
	case UCL_FLOAT:
		printf("\"t\":\"float\",\"v\":\"%.17g\"", ucl_object_todouble(obj));
		break;
	case UCL_TIME:
		printf("\"t\":\"time\",\"v\":\"%.17g\"", ucl_object_todouble(obj));
		break;
	case UCL_STRING:
		s = ucl_object_tolstring(obj, &len);
		fputs("\"t\":\"string\",", stdout);
		put_bytes_field("v", "hex", (const unsigned char *) s, len);
		break;
	case UCL_BOOLEAN:
		printf("\"t\":\"bool\",\"v\":%s", ucl_object_toboolean(obj) ? "true" : "false");
		break;
	case UCL_NULL:
		fputs("\"t\":\"null\"", stdout);
		break;
	case UCL_USERDATA:
		fputs("\"t\":\"userdata\"", stdout);
		break;
	default:
		printf("\"t\":\"unknown-%u\"", (unsigned) obj->type);
		break;
	}
	pri = ucl_object_get_priority(obj);
	if (pri != 0) {
		printf(",\"pri\":%u", pri);
	}
	putchar('}');
}

static unsigned char *
read_file(const char *path, size_t *out_len)
{
	FILE *f = fopen(path, "rb");
	unsigned char *buf = NULL;
	size_t cap = 0, len = 0;

	if (f == NULL) {
		return NULL;
	}
	for (;;) {
		if (len == cap) {
			cap = cap ? cap * 2 : 8192;
			buf = realloc(buf, cap);
			if (buf == NULL) {
				fclose(f);
				return NULL;
			}
		}
		size_t r = fread(buf + len, 1, cap - len, f);
		len += r;
		if (r == 0) {
			break;
		}
	}
	if (ferror(f)) {
		free(buf);
		fclose(f);
		return NULL;
	}
	fclose(f);
	*out_len = len;
	return buf;
}

int
main(int argc, char **argv)
{
	int opt, flags = UCL_PARSER_DEFAULT;
	bool filevars = true;
	struct ucl_parser *parser;
	unsigned char *buf;
	size_t len = 0;
	ucl_object_t *top;

	while ((opt = getopt(argc, argv, "lzTICMF")) != -1) {
		switch (opt) {
		case 'l':
			flags |= UCL_PARSER_KEY_LOWERCASE;
			break;
		case 'z':
			flags |= UCL_PARSER_ZEROCOPY;
			break;
		case 'T':
			flags |= UCL_PARSER_NO_TIME;
			break;
		case 'I':
			flags |= UCL_PARSER_NO_IMPLICIT_ARRAYS;
			break;
		case 'C':
			flags |= UCL_PARSER_SAVE_COMMENTS;
			break;
		case 'M':
			flags |= UCL_PARSER_DISABLE_MACRO;
			break;
		case 'F':
			flags |= UCL_PARSER_NO_FILEVARS;
			filevars = false;
			break;
		default:
			fprintf(stderr, "usage: %s [-lzTICMF] <file>\n", argv[0]);
			return 2;
		}
	}
	if (optind != argc - 1) {
		fprintf(stderr, "usage: %s [-lzTICMF] <file>\n", argv[0]);
		return 2;
	}

	buf = read_file(argv[optind], &len);
	if (buf == NULL) {
		perror(argv[optind]);
		return 2;
	}

	parser = ucl_parser_new(flags);
	if (parser == NULL) {
		fprintf(stderr, "cannot create parser\n");
		return 2;
	}
	ucl_parser_register_variable(parser, "ABI", "unknown");
	if (filevars) {
		ucl_parser_set_filevars(parser, argv[optind], true);
	}

	ucl_parser_add_chunk_full(parser, buf, len, 0, UCL_DUPLICATE_APPEND, UCL_PARSE_UCL);

	if (ucl_parser_get_error(parser) != NULL) {
		fprintf(stderr, "libucl: %s\n", ucl_parser_get_error(parser));
		fputs("{\"error\":true}\n", stdout);
	}
	else {
		top = ucl_parser_get_object(parser);
		if (top == NULL) {
			fputs("{\"error\":true}\n", stdout);
		}
		else {
			dump_node(top);
			putchar('\n');
			ucl_object_unref(top);
		}
	}

	ucl_parser_free(parser);
	free(buf);
	return 0;
}

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
 *   With -c, a node that has saved comments attached also carries
 *   "c":["<comment>",...] (comments libucl's config output writes before the
 *   value) or "ca":[...] (comments it writes after the value), in order, each
 *   exactly as it appears in the input.
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
 *   - the input is added as one chunk, priority 0, UCL_DUPLICATE_APPEND
 *     (changeable with -p and -s).
 *
 * Usage: ucl-dump [-l] [-z] [-T] [-I] [-C] [-M] [-F] [-e FORMAT] <file>
 *   -l  UCL_PARSER_KEY_LOWERCASE      -z  UCL_PARSER_ZEROCOPY
 *   -T  UCL_PARSER_NO_TIME            -I  UCL_PARSER_NO_IMPLICIT_ARRAYS
 *   -C  UCL_PARSER_SAVE_COMMENTS      -M  UCL_PARSER_DISABLE_MACRO
 *   -F  UCL_PARSER_NO_FILEVARS (and do not set file variables)
 *   -S  do not set the file variables from the input path, as for a document
 *       given as a string (libucl then defines FILENAME as "undef" and
 *       CURDIR as the working directory, unless -F)
 *   -v NAME=VALUE  register an extra variable (after ABI, before the file
 *       variables); may be repeated, registration order is kept
 *   -H  install a variable handler that resolves any name starting with "H_"
 *       to the text "[handled]" and refuses every other name
 *   -p N  add the input chunk with priority N (default 0)
 *   -s STRATEGY  add the input chunk with duplicate strategy STRATEGY:
 *       append (default), merge, rewrite or error
 *   -c  save comments (implies -C) and add them to the typed dump as "c"/"ca"
 *   -e  instead of the typed dump, print libucl's own output for the parsed
 *       object in FORMAT: config, json, json-compact or yaml (exact bytes, as
 *       ucl_object_emit returns them). A parse failure prints "error\n".
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

/* Comments saved by the parser, when -c is given. */
static const ucl_object_t *g_comments = NULL;

static void
dump_comments(const ucl_object_t *obj)
{
	const ucl_object_t *cm, *cur;
	bool first = true;

	if (g_comments == NULL) {
		return;
	}
	cm = ucl_comments_find(g_comments, obj);
	if (cm == NULL) {
		return;
	}
	/* An inherited-flagged comment list is written after the value */
	printf(",\"%s\":[", (cm->flags & UCL_OBJECT_INHERITED) ? "ca" : "c");
	for (cur = cm; cur != NULL; cur = cur->next) {
		size_t len = 0;
		const char *s = ucl_object_tolstring(cur, &len);

		if (!first) {
			putchar(',');
		}
		first = false;
		put_json_string((const unsigned char *) s, len);
	}
	putchar(']');
}

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
	dump_comments(obj);
	putchar('}');
}

static bool
test_var_handler(const unsigned char *data, size_t len, unsigned char **replace,
				 size_t *replace_len, bool *need_free, void *ud)
{
	static const char text[] = "[handled]";

	(void) ud;
	if (len >= 2 && data[0] == 'H' && data[1] == '_') {
		*replace = (unsigned char *) text;
		*replace_len = sizeof(text) - 1;
		*need_free = false;
		return true;
	}
	return false;
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
	int emit = -1;
	const char *vars[64];
	int nvars = 0;
	bool handler = false;
	bool dump_comm = false;
	unsigned priority = 0;
	enum ucl_duplicate_strategy strat = UCL_DUPLICATE_APPEND;
	struct ucl_parser *parser;
	unsigned char *buf;
	size_t len = 0;
	ucl_object_t *top;

	while ((opt = getopt(argc, argv, "lzTICMFSce:v:Hp:s:")) != -1) {
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
		case 'c':
			flags |= UCL_PARSER_SAVE_COMMENTS;
			dump_comm = true;
			break;
		case 'F':
			flags |= UCL_PARSER_NO_FILEVARS;
			filevars = false;
			break;
		case 'v':
			if (nvars < 64 && strchr(optarg, '=') != NULL) {
				vars[nvars++] = optarg;
			}
			break;
		case 'H':
			handler = true;
			break;
		case 'S':
			filevars = false;
			break;
		case 'p':
			priority = (unsigned) strtoul(optarg, NULL, 10);
			break;
		case 's':
			if (strcmp(optarg, "append") == 0) {
				strat = UCL_DUPLICATE_APPEND;
			}
			else if (strcmp(optarg, "merge") == 0) {
				strat = UCL_DUPLICATE_MERGE;
			}
			else if (strcmp(optarg, "rewrite") == 0) {
				strat = UCL_DUPLICATE_REWRITE;
			}
			else if (strcmp(optarg, "error") == 0) {
				strat = UCL_DUPLICATE_ERROR;
			}
			else {
				fprintf(stderr, "unknown strategy '%s'\n", optarg);
				return 2;
			}
			break;
		case 'e':
			if (strcmp(optarg, "config") == 0) {
				emit = UCL_EMIT_CONFIG;
			}
			else if (strcmp(optarg, "json") == 0) {
				emit = UCL_EMIT_JSON;
			}
			else if (strcmp(optarg, "json-compact") == 0) {
				emit = UCL_EMIT_JSON_COMPACT;
			}
			else if (strcmp(optarg, "yaml") == 0) {
				emit = UCL_EMIT_YAML;
			}
			else {
				fprintf(stderr, "unknown format '%s'\n", optarg);
				return 2;
			}
			break;
		default:
			fprintf(stderr, "usage: %s [-lzTICMFScH] [-v NAME=VALUE] [-p N] [-s STRATEGY] [-e FORMAT] <file>\n", argv[0]);
			return 2;
		}
	}
	if (optind != argc - 1) {
		fprintf(stderr, "usage: %s [-lzTICMFScH] [-v NAME=VALUE] [-p N] [-s STRATEGY] [-e FORMAT] <file>\n", argv[0]);
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
	for (int i = 0; i < nvars; i++) {
		char name[256];
		const char *eq = strchr(vars[i], '=');
		size_t nlen = (size_t) (eq - vars[i]);

		if (nlen >= sizeof(name)) {
			nlen = sizeof(name) - 1;
		}
		memcpy(name, vars[i], nlen);
		name[nlen] = '\0';
		ucl_parser_register_variable(parser, name, eq + 1);
	}
	if (handler) {
		ucl_parser_set_variables_handler(parser, test_var_handler, NULL);
	}
	if (filevars) {
		ucl_parser_set_filevars(parser, argv[optind], true);
	}

	ucl_parser_add_chunk_full(parser, buf, len, priority, strat, UCL_PARSE_UCL);

	if (ucl_parser_get_error(parser) != NULL) {
		fprintf(stderr, "libucl: %s\n", ucl_parser_get_error(parser));
		fputs(emit >= 0 ? "error\n" : "{\"error\":true}\n", stdout);
	}
	else if (emit >= 0) {
		top = ucl_parser_get_object(parser);
		if (top == NULL) {
			fputs("error\n", stdout);
		}
		else {
			unsigned char *out = ucl_object_emit(top, (enum ucl_emitter) emit);
			if (out != NULL) {
				fputs((const char *) out, stdout);
				free(out);
			}
			ucl_object_unref(top);
		}
	}
	else {
		top = ucl_parser_get_object(parser);
		if (top == NULL) {
			fputs("{\"error\":true}\n", stdout);
		}
		else {
			if (dump_comm) {
				g_comments = ucl_parser_get_comments(parser);
			}
			dump_node(top);
			putchar('\n');
			ucl_object_unref(top);
		}
	}

	ucl_parser_free(parser);
	free(buf);
	return 0;
}

/* Copyright (c) 2013-2015, Vsevolod Stakhov
 * All rights reserved.
 *
 * Redistribution and use in source and binary forms, with or without
 * modification, are permitted provided that the following conditions are met:
 *       * Redistributions of source code must retain the above copyright
 *         notice, this list of conditions and the following disclaimer.
 *       * Redistributions in binary form must reproduce the above copyright
 *         notice, this list of conditions and the following disclaimer in the
 *         documentation and/or other materials provided with the distribution.
 *
 * THIS SOFTWARE IS PROVIDED ''AS IS'' AND ANY
 * EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE IMPLIED
 * WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
 * DISCLAIMED. IN NO EVENT SHALL AUTHOR BE LIABLE FOR ANY
 * DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES
 * (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES;
 * LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND
 * ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT
 * (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF THIS
 * SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
 */

/* C15 STAGE A, spec-v22: released public declarations only.
 * ABI declarations derived from libucl's public include/ucl.h at
 * 24c8b399062ae4691168c243e3b7345ef7f31956.
 * No upstream executable function bodies are included.
 */
#ifndef SERDE_UCL_C15_H
#define SERDE_UCL_C15_H
#include <stddef.h>
#include <stdint.h>
#include <stdbool.h>
#include <stdio.h>
#ifdef __cplusplus
extern "C" {
#endif




typedef enum ucl_error {
	UCL_EOK = 0,
	UCL_ESYNTAX,
	UCL_EIO,
	UCL_ESTATE,
	UCL_ENESTED,
	UCL_EUNPAIRED,
	UCL_EMACRO,
	UCL_EINTERNAL,
	UCL_ESSL,
	UCL_EMERGE,
	UCL_ELIMIT
} ucl_error_t;

typedef enum ucl_type {
	UCL_OBJECT = 0,
	UCL_ARRAY,
	UCL_INT,
	UCL_FLOAT,
	UCL_STRING,
	UCL_BOOLEAN,
	UCL_TIME,
	UCL_USERDATA,
	UCL_NULL
} ucl_type_t;

typedef enum ucl_emitter {
	UCL_EMIT_JSON = 0,
	UCL_EMIT_JSON_COMPACT,
	UCL_EMIT_CONFIG,
	UCL_EMIT_YAML,
	UCL_EMIT_MSGPACK,
	UCL_EMIT_MAX
} ucl_emitter_t;

typedef enum ucl_parser_flags {
	UCL_PARSER_DEFAULT = 0,
	UCL_PARSER_KEY_LOWERCASE = (1 << 0),
	UCL_PARSER_ZEROCOPY = (1 << 1),
	UCL_PARSER_NO_TIME = (1 << 2),
	UCL_PARSER_NO_IMPLICIT_ARRAYS = (1 << 3),
	UCL_PARSER_SAVE_COMMENTS = (1 << 4),
	UCL_PARSER_DISABLE_MACRO = (1 << 5),
	UCL_PARSER_NO_FILEVARS = (1 << 6)
} ucl_parser_flags_t;

typedef enum ucl_string_flags {
	UCL_STRING_RAW = 0x0,
	UCL_STRING_ESCAPE = (1 << 0),
	UCL_STRING_TRIM = (1 << 1),
	UCL_STRING_PARSE_BOOLEAN = (1 << 2),
	UCL_STRING_PARSE_INT = (1 << 3),
	UCL_STRING_PARSE_DOUBLE = (1 << 4),
	UCL_STRING_PARSE_TIME = (1 << 5),
	UCL_STRING_PARSE_NUMBER = UCL_STRING_PARSE_INT | UCL_STRING_PARSE_DOUBLE | UCL_STRING_PARSE_TIME,
	UCL_STRING_PARSE = UCL_STRING_PARSE_BOOLEAN | UCL_STRING_PARSE_NUMBER,
	UCL_STRING_PARSE_BYTES = (1 << 6)
} ucl_string_flags_t;

typedef enum ucl_object_flags {
	UCL_OBJECT_ALLOCATED_KEY = (1 << 0),
	UCL_OBJECT_ALLOCATED_VALUE = (1 << 1),
	UCL_OBJECT_NEED_KEY_ESCAPE = (1 << 2),
	UCL_OBJECT_EPHEMERAL = (1 << 3),
	UCL_OBJECT_MULTILINE = (1 << 4),
	UCL_OBJECT_MULTIVALUE = (1 << 5),
	UCL_OBJECT_INHERITED = (1 << 6),
	UCL_OBJECT_BINARY = (1 << 7),
	UCL_OBJECT_SQUOTED = (1 << 8)
} ucl_object_flags_t;

enum ucl_duplicate_strategy {
	UCL_DUPLICATE_APPEND = 0,
	UCL_DUPLICATE_MERGE,
	UCL_DUPLICATE_REWRITE,
	UCL_DUPLICATE_ERROR
};

enum ucl_parse_type {
	UCL_PARSE_UCL = 0,
	UCL_PARSE_MSGPACK,
	UCL_PARSE_CSEXP,
	UCL_PARSE_AUTO
};

enum ucl_object_keys_sort_flags {
	UCL_SORT_KEYS_DEFAULT = 0,
	UCL_SORT_KEYS_ICASE = (1u << 0u),
	UCL_SORT_KEYS_RECURSIVE = (1u << 1u),
};

enum ucl_iterate_type {
	UCL_ITERATE_EXPLICIT = 1 << 0,
	UCL_ITERATE_IMPLICIT = 1 << 1,
	UCL_ITERATE_BOTH = (1 << 0) | (1 << 1),
};

struct ucl_emitter_functions {

	int (*ucl_emitter_append_character)(unsigned char c, size_t nchars, void *ud);

	int (*ucl_emitter_append_len)(unsigned const char *str, size_t len, void *ud);

	int (*ucl_emitter_append_int)(int64_t elt, void *ud);

	int (*ucl_emitter_append_double)(double elt, void *ud);

	void (*ucl_emitter_free_func)(void *ud);

	void *ud;
};

typedef struct ucl_object_s {

	union {
		int64_t iv;
		const char *sv;
		double dv;
		void *av;
		void *ov;
		void *ud;
	} value;
	const char *key;
	struct ucl_object_s *next;
	struct ucl_object_s *prev;
	uint32_t keylen;
	uint32_t len;
	uint32_t ref;
	uint16_t flags;
	uint16_t type;
	unsigned char *trash_stack[2];
} ucl_object_t;

struct ucl_parser;

typedef void *ucl_object_iter_t;

typedef bool (*ucl_macro_handler)(const unsigned char *data, size_t len, const ucl_object_t *arguments, void *ud);

typedef bool (*ucl_context_macro_handler)(const unsigned char *data, size_t len, const ucl_object_t *arguments, const ucl_object_t *context, void *ud);

typedef bool (*ucl_variable_handler)(const unsigned char *data, size_t len, unsigned char **replace, size_t *replace_len, bool *need_free, void *ud);

ucl_type_t ucl_object_type(const ucl_object_t *obj);

const char *ucl_object_type_to_string(ucl_type_t type);

bool ucl_object_string_to_type(const char *input, ucl_type_t *res);

unsigned int ucl_array_size(const ucl_object_t *top);

const ucl_object_t *ucl_array_find_index(const ucl_object_t *top, unsigned int index);

bool ucl_object_todouble_safe(const ucl_object_t *obj, double *target);

double ucl_object_todouble(const ucl_object_t *obj);

bool ucl_object_toint_safe(const ucl_object_t *obj, int64_t *target);

int64_t ucl_object_toint(const ucl_object_t *obj);

bool ucl_object_toboolean_safe(const ucl_object_t *obj, bool *target);

bool ucl_object_toboolean(const ucl_object_t *obj);

bool ucl_object_tostring_safe(const ucl_object_t *obj, const char **target);

const char *ucl_object_tostring(const ucl_object_t *obj);

const char *ucl_object_tostring_forced(const ucl_object_t *obj);

bool ucl_object_tolstring_safe(const ucl_object_t *obj, const char **target, size_t *tlen);

const char *ucl_object_tolstring(const ucl_object_t *obj, size_t *tlen);

const ucl_object_t *ucl_object_lookup(const ucl_object_t *obj, const char *key);

const ucl_object_t *ucl_object_lookup_len(const ucl_object_t *obj, const char *key, size_t klen);

const char *ucl_object_key(const ucl_object_t *obj);

const char *ucl_object_keyl(const ucl_object_t *obj, size_t *len);

ucl_object_t *ucl_object_ref(const ucl_object_t *obj);

void ucl_object_unref(ucl_object_t *obj);

const ucl_object_t *ucl_object_iterate_with_error(const ucl_object_t *obj, ucl_object_iter_t *iter, bool expand_values, int *ep);

void ucl_object_iterate_end(const ucl_object_t *obj, ucl_object_iter_t *iter);

ucl_object_iter_t ucl_object_iterate_new(const ucl_object_t *obj) ;

bool ucl_object_iter_chk_excpn(ucl_object_iter_t *it);

ucl_object_iter_t ucl_object_iterate_reset(ucl_object_iter_t it, const ucl_object_t *obj);

const ucl_object_t *ucl_object_iterate_safe(ucl_object_iter_t iter, bool expand_values);

const ucl_object_t *ucl_object_iterate_full(ucl_object_iter_t iter, enum ucl_iterate_type type);

void ucl_object_iterate_free(ucl_object_iter_t it);

struct ucl_parser *ucl_parser_new(int flags);

void ucl_parser_register_variable(struct ucl_parser *parser, const char *var, const char *value);

bool ucl_parser_add_chunk(struct ucl_parser *parser, const unsigned char *data, size_t len);

bool ucl_parser_add_string(struct ucl_parser *parser, const char *data, size_t len);

bool ucl_parser_add_file(struct ucl_parser *parser, const char *filename);

ucl_object_t *ucl_parser_get_object(struct ucl_parser *parser);

const char *ucl_parser_get_error(struct ucl_parser *parser);

int ucl_parser_get_error_code(struct ucl_parser *parser);

unsigned ucl_parser_get_column(struct ucl_parser *parser);

unsigned ucl_parser_get_linenum(struct ucl_parser *parser);

void ucl_parser_free(struct ucl_parser *parser);

unsigned char *ucl_object_emit(const ucl_object_t *obj, enum ucl_emitter emit_type);

unsigned char *ucl_object_emit_len(const ucl_object_t *obj, enum ucl_emitter emit_type, size_t *len);

#define ucl_object_find_key ucl_object_lookup


#define ucl_object_find_keyl ucl_object_lookup_len




#define ucl_iterate_object ucl_object_iterate
#define ucl_object_iterate(ob, it, ev) ucl_object_iterate_with_error((ob), (it), (ev), NULL)

#define ucl_iterate_object_end ucl_object_iterate_end

#define ucl_obj_todouble_safe ucl_object_todouble_safe

#define ucl_obj_todouble ucl_object_todouble

#define ucl_obj_tostring ucl_object_tostring

#define ucl_obj_tostring_safe ucl_object_tostring_safe

#define ucl_obj_tolstring ucl_object_tolstring

#define ucl_obj_tolstring_safe ucl_object_tolstring_safe

#define ucl_obj_toint ucl_object_toint

#define ucl_obj_toint_safe ucl_object_toint_safe

#define ucl_obj_toboolean ucl_object_toboolean

#define ucl_obj_toboolean_safe ucl_object_toboolean_safe

#define ucl_obj_get_key ucl_object_find_key

#define ucl_obj_get_keyl ucl_object_find_keyl

#define ucl_obj_unref ucl_object_unref

#define ucl_obj_ref ucl_object_ref


#define UCL_PRIORITY_MIN 0

#define UCL_PRIORITY_MAX 15

#define UCL_PARSER_SAFE_FLAGS (UCL_PARSER_NO_TIME | UCL_PARSER_NO_IMPLICIT_ARRAYS | UCL_PARSER_DISABLE_MACRO | UCL_PARSER_NO_FILEVARS)

#ifdef __cplusplus
}
#endif
#endif

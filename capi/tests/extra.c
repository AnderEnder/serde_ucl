/* Implementation-side assertions from the released Stage A contract. */
#define _DARWIN_C_SOURCE
#define _DEFAULT_SOURCE
#include <ucl.h>
#include <assert.h>
#include <pthread.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/mman.h>
#include <unistd.h>

static ucl_object_t *parse(const char *text) {
    struct ucl_parser *parser = ucl_parser_new(0);
    assert(parser && ucl_parser_add_string(parser, text, 0));
    ucl_object_t *root = ucl_parser_get_object(parser);
    assert(root);
    ucl_parser_free(parser);
    return root;
}

static void boundary(void) {
    long page_size = sysconf(_SC_PAGESIZE);
    assert(page_size > 0);
    void *inaccessible = mmap(NULL, (size_t)page_size, PROT_NONE,
                              MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
    assert(inaccessible != MAP_FAILED);
    const char *inputs[] = {"a=1", "a=\"open", "a={",
                            ".try_include \"nonexistent-c15-extra.ucl\"\na=2", ""};
    for (size_t i = 0; i < sizeof(inputs) / sizeof(*inputs); ++i) {
        for (int initial = 0; initial < 2; ++initial) {
            for (int second = 0; second < 3; ++second) {
                struct ucl_parser *parser = ucl_parser_new(0);
                assert(parser);
                bool first = initial
                    ? ucl_parser_add_string(parser, inputs[i], 0)
                    : ucl_parser_add_chunk(parser, (const unsigned char *)inputs[i], strlen(inputs[i]));
                assert(first == (i == 0 || i == 4));
                ucl_object_t *before = ucl_parser_get_object(parser);
                unsigned char *saved = before ? ucl_object_emit(before, UCL_EMIT_JSON_COMPACT) : NULL;
                bool accepted = second == 0
                    ? ucl_parser_add_chunk(parser, inaccessible, 1)
                    : second == 1 ? ucl_parser_add_string(parser, inaccessible, 0)
                                  : ucl_parser_add_file(parser, inaccessible);
                assert(!accepted);
                assert(ucl_parser_get_error_code(parser) == UCL_ESTATE);
                const char *error = ucl_parser_get_error(parser);
                assert(error && error[0]);
                ucl_object_t *after = ucl_parser_get_object(parser);
                assert(after == before);
                if (after) {
                    unsigned char *actual = ucl_object_emit(after, UCL_EMIT_JSON_COMPACT);
                    assert(saved && actual && strcmp((const char *)saved, (const char *)actual) == 0);
                    free(actual);
                }
                free(saved);
                ucl_object_unref(after);
                ucl_object_unref(before);
                ucl_parser_free(parser);
            }
        }
    }
    for (int missing = 0; missing < 2; ++missing) {
        const char *path = "c15-extra-main.ucl";
        if (!missing) {
            FILE *file = fopen(path, "wb");
            assert(file && fputs("a=1", file) >= 0 && fclose(file) == 0);
        }
        struct ucl_parser *parser = ucl_parser_new(0);
        assert(parser && ucl_parser_add_file(parser, path) == !missing);
        assert(!ucl_parser_add_file(parser, inaccessible));
        assert(ucl_parser_get_error_code(parser) == UCL_ESTATE);
        assert(ucl_parser_get_error(parser) && ucl_parser_get_error(parser)[0]);
        ucl_parser_free(parser);
        if (!missing) assert(unlink(path) == 0);
    }
    assert(munmap(inaccessible, (size_t)page_size) == 0);
}

static void lifetime(void) {
    for (int order = 0; order < 2; ++order) {
        struct ucl_parser *parser = ucl_parser_new(0);
        assert(parser && ucl_parser_add_string(parser,
               "child={text=\"hello\"; ar=[1,{x=2}]; n=1.75}; sibling=0", 0));
        ucl_object_t *root = ucl_parser_get_object(parser);
        ucl_object_t *child = ucl_object_ref(ucl_object_lookup(root, "child"));
        const ucl_object_t *text = ucl_object_lookup(child, "text");
        const char *borrowed = ucl_object_tostring(text);
        const ucl_object_t *number = ucl_object_lookup(child, "n");
        const char *forced = ucl_object_tostring_forced(number);
        assert(strcmp(forced, "1.750000") == 0);
        if (order) { ucl_parser_free(parser); ucl_object_unref(root); }
        else { ucl_object_unref(root); ucl_parser_free(parser); }
        assert(child->ref == 1);
        assert(strcmp(borrowed, "hello") == 0 && borrowed[5] == '\0');
        (void)ucl_object_tostring_forced(text);
        assert(strcmp(forced, "1.750000") == 0);
        assert(ucl_object_toint(ucl_object_lookup(
            ucl_array_find_index(ucl_object_lookup(child, "ar"), 1), "x")) == 2);
        ucl_object_iter_t old = NULL;
        assert(ucl_object_iterate(child, &old, true) == text);
        ucl_object_iterate_end(child, &old);
        assert(old == NULL);
        ucl_object_iter_t safe = ucl_object_iterate_new(child);
        assert(safe);
        for (int pass = 0; pass < 3; ++pass) {
            size_t count = 0;
            while (ucl_object_iterate_safe(safe, true)) ++count;
            assert(count == 3);
            assert(!ucl_object_iter_chk_excpn((ucl_object_iter_t *)safe));
            safe = ucl_object_iterate_reset(safe, child);
            assert(safe);
        }
        ucl_object_iterate_free(safe);
        for (int format = 0; format < 4; ++format) {
            size_t length = 0;
            unsigned char *out = ucl_object_emit_len(child, (ucl_emitter_t)format, &length);
            assert(out && length > 0 && out[length] == '\0');
            free(out);
        }
        ucl_object_unref(child);
    }
}

static void *deep(void *argument) {
    (void)argument;
    const size_t depth = 1023;
    for (int objects = 0; objects < 2; ++objects) {
        size_t capacity = depth * 5 + 16;
        char *input = malloc(capacity);
        assert(input);
        size_t used = 0;
        memcpy(input, "x=", 2); used = 2;
        for (size_t i = 0; i < depth; ++i) {
            if (objects) { memcpy(input + used, "{x=", 3); used += 3; }
            else input[used++] = '[';
        }
        input[used++] = '0';
        for (size_t i = 0; i < depth; ++i) input[used++] = objects ? '}' : ']';
        input[used] = '\0';
        assert(used < capacity);
        ucl_object_t *root = parse(input);
        free(input);
        ucl_object_t *retained = ucl_object_ref(ucl_object_lookup(root, "x"));
        ucl_object_unref(root);
        const ucl_object_t *value = retained;
        for (size_t i = 0; i < depth; ++i) {
            assert(ucl_object_type(value) == (objects ? UCL_OBJECT : UCL_ARRAY));
            value = objects ? ucl_object_lookup(value, "x") : ucl_array_find_index(value, 0);
            assert(value);
        }
        assert(ucl_object_type(value) == UCL_INT && ucl_object_toint(value) == 0);
        for (int format = 0; format < 4; ++format) {
            size_t length = 0;
            unsigned char *out = ucl_object_emit_len(retained, (ucl_emitter_t)format, &length);
            assert(out && length && out[length] == '\0');
            free(out);
        }
        ucl_object_unref(retained);
    }
    return NULL;
}

int main(void) {
    boundary();
    lifetime();
    pthread_attr_t attributes;
    assert(pthread_attr_init(&attributes) == 0);
    assert(pthread_attr_setstacksize(&attributes, 2 * 1024 * 1024) == 0);
    pthread_t thread;
    assert(pthread_create(&thread, &attributes, deep, NULL) == 0);
    assert(pthread_attr_destroy(&attributes) == 0);
    assert(pthread_join(thread, NULL) == 0);
    puts("C boundary, retained lifetimes, iterator cleanup, and 1023-container depth passed");
    return 0;
}

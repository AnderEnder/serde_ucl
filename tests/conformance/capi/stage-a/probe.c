/* C15 Stage A black-box cases, spec-team authored. Public interface only. */
#include <ucl.h>
#include <assert.h>
#include <inttypes.h>
#include <stddef.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <sys/stat.h>

static void bytes(const char *label,const char *p,size_t n) {
    printf("%s[%zu]=",label,n);
    if(p) for(size_t i=0;i<n;i++) printf("%02x",(unsigned char)p[i]);
    else printf("<null>");
    putchar('\n');
}
static void dump(const char *label,const ucl_object_t *o) {
    size_t n=0;
    unsigned char *s=ucl_object_emit_len(o,UCL_EMIT_JSON_COMPACT,&n);
    bytes(label,(const char *)s,n);free(s);
}
static ucl_object_t *parse(const char *s) {
    struct ucl_parser *p=ucl_parser_new(0);assert(p);
    assert(ucl_parser_add_string(p,s,0));
    ucl_object_t *o=ucl_parser_get_object(p);assert(o);
    ucl_parser_free(p);return o;
}
static void abi(void) {
#define SIZE(t) printf(#t ":size=%zu,align=%zu\n",sizeof(t),_Alignof(t))
#define FIELD(f) printf("ucl_object_t." #f "=%zu\n",offsetof(ucl_object_t,f))
    SIZE(ucl_object_t);SIZE(ucl_object_iter_t);SIZE(ucl_type_t);SIZE(ucl_error_t);SIZE(ucl_emitter_t);
    FIELD(value);FIELD(key);FIELD(next);FIELD(prev);FIELD(keylen);FIELD(len);FIELD(ref);FIELD(flags);FIELD(type);FIELD(trash_stack);
    printf("types=%d,%d,%d,%d,%d,%d,%d,%d,%d\n",UCL_OBJECT,UCL_ARRAY,UCL_INT,UCL_FLOAT,UCL_STRING,UCL_BOOLEAN,UCL_TIME,UCL_USERDATA,UCL_NULL);
    printf("emit=%d,%d,%d,%d,%d,%d\n",UCL_EMIT_JSON,UCL_EMIT_JSON_COMPACT,UCL_EMIT_CONFIG,UCL_EMIT_YAML,UCL_EMIT_MSGPACK,UCL_EMIT_MAX);
    printf("parser=%d,%d,%d,%d,%d,%d,%d,%d,%d\n",UCL_PARSER_DEFAULT,UCL_PARSER_KEY_LOWERCASE,UCL_PARSER_ZEROCOPY,UCL_PARSER_NO_TIME,UCL_PARSER_NO_IMPLICIT_ARRAYS,UCL_PARSER_SAVE_COMMENTS,UCL_PARSER_DISABLE_MACRO,UCL_PARSER_NO_FILEVARS,UCL_PARSER_SAFE_FLAGS);
    printf("flags=%d,%d,%d,%d,%d,%d,%d,%d,%d\n",UCL_OBJECT_ALLOCATED_KEY,UCL_OBJECT_ALLOCATED_VALUE,UCL_OBJECT_NEED_KEY_ESCAPE,UCL_OBJECT_EPHEMERAL,UCL_OBJECT_MULTILINE,UCL_OBJECT_MULTIVALUE,UCL_OBJECT_INHERITED,UCL_OBJECT_BINARY,UCL_OBJECT_SQUOTED);
    printf("errors=%d,%d,%d,%d,%d,%d,%d,%d,%d,%d,%d\n",UCL_EOK,UCL_ESYNTAX,UCL_EIO,UCL_ESTATE,UCL_ENESTED,UCL_EUNPAIRED,UCL_EMACRO,UCL_EINTERNAL,UCL_ESSL,UCL_EMERGE,UCL_ELIMIT);
    printf("iteration=%d,%d,%d;priority=%d,%d\n",UCL_ITERATE_EXPLICIT,UCL_ITERATE_IMPLICIT,UCL_ITERATE_BOTH,UCL_PRIORITY_MIN,UCL_PRIORITY_MAX);
}
static void diagnostic(const char *label,struct ucl_parser *p,bool ok) {
    printf("%s:ok=%d,code=%d,message=%d,line=%u,column=%u\n",label,ok,ucl_parser_get_error_code(p),ucl_parser_get_error(p)!=NULL,ucl_parser_get_linenum(p),ucl_parser_get_column(p));
    ucl_object_t *o=ucl_parser_get_object(p);printf("root-present=%d\n",o!=NULL);
    if(o) dump("root",o);
    ucl_object_unref(o);
}
static void parser(void) {
    const char *inputs[]={""," ","a=1","a=1\nb=2","{a=1} trailing ignored","a=\"open","a={","a=[", ".unknown x",".try_include \"missing.ucl\"\na=2",".include \"missing.ucl\""};
    for(size_t i=0;i<sizeof(inputs)/sizeof(*inputs);i++) for(int mode=0;mode<2;mode++) {
        struct ucl_parser *p=ucl_parser_new(0);assert(p);
        if(i==0 && mode==0) diagnostic("before",p,true);
        bool ok=mode ? ucl_parser_add_string(p,inputs[i],0) : ucl_parser_add_chunk(p,(const unsigned char *)inputs[i],strlen(inputs[i]));
        printf("input=%zu,mode=%d\n",i,mode);diagnostic("after",p,ok);ucl_parser_free(p);
    }
    struct ucl_parser *p=ucl_parser_new(UCL_PARSER_NO_FILEVARS);
    ucl_parser_register_variable(p,"V","first");ucl_parser_register_variable(p,"V","second");
    assert(ucl_parser_add_string(p,"a=\"$V\"; missing=\"${UNKNOWN}\"",0));
    ucl_object_t *o=ucl_parser_get_object(p);dump("registered",o);ucl_object_unref(o);ucl_parser_free(p);
    p=ucl_parser_new(0);assert(ucl_parser_add_chunk(p,(const unsigned char *)"a=1garbage",3));
    o=ucl_parser_get_object(p);dump("bounded",o);ucl_object_unref(o);ucl_parser_free(p);
}
static void metadata(void) {
    const char *text="a=1; a=2; a=3; quoted='x'; str=\"é\"; n=null; b=true; t=2s; f=1.5; ar=[1,{x=2}]; ob={x=1}; \"x y\"=5; multi=<<E\nhello\nE\n";
    for(int mode=0;mode<2;mode++) {
        struct ucl_parser *p=ucl_parser_new(mode?UCL_PARSER_ZEROCOPY:0);
        assert(p && ucl_parser_add_string(p,text,0));ucl_object_t *o=ucl_parser_get_object(p);assert(o);
        printf("mode=%d,root:type=%u,len=%u,ref=%u,flags=%u,keylen=%u,prev-self=%d,next-null=%d\n",mode,o->type,o->len,o->ref,o->flags,o->keylen,o->prev==o,o->next==NULL);
        ucl_object_iter_t it=NULL;const ucl_object_t *head;
        while((head=ucl_object_iterate(o,&it,true))) {
            size_t n=0;const char *k=ucl_object_keyl(head,&n);bytes("key",k,n);
            size_t index=0;
            for(const ucl_object_t *v=head;v;v=v->next,index++) {
                printf("value=%zu,type=%u,len=%u,ref=%u,flags=%u,keylen=%u,prev-head=%d,prev-self=%d,next=%d\n",index,v->type,v->len,v->ref,v->flags,v->keylen,v->prev==head,v->prev==v,v->next!=NULL);
                if(v->type==UCL_STRING)bytes("string",v->value.sv,v->len);
                if(v->type==UCL_INT || v->type==UCL_BOOLEAN)printf("iv=%" PRId64 "\n",v->value.iv);
                if(v->type==UCL_FLOAT || v->type==UCL_TIME)printf("dv=%.17g\n",v->value.dv);
            }
        }
        const ucl_object_t *a=ucl_object_lookup(o,"a");printf("chain:tail=%d,middle-prev-head=%d,last-prev-middle=%d\n",a->prev==a->next->next,a->next->prev==a,a->next->next->prev==a->next);
        const ucl_object_t *ar=ucl_object_lookup(o,"ar");
        for(unsigned int j=0;j<ucl_array_size(ar);j++) {
            const ucl_object_t *v=ucl_array_find_index(ar,j);
            printf("array-element=%u,type=%u,len=%u,ref=%u,flags=%u,keylen=%u,prev-self=%d,next-null=%d\n",j,v->type,v->len,v->ref,v->flags,v->keylen,v->prev==v,v->next==NULL);
        }
        ucl_object_unref(o);ucl_parser_free(p);
    }
    ucl_object_t *o=parse("d {x=1} e {.inherit \"d\"}");
    const ucl_object_t *x=ucl_object_lookup(ucl_object_lookup(o,"e"),"x");printf("inherited:flags=%u,len=%u\n",x->flags,x->len);ucl_object_unref(o);
    o=parse(".priority 3\na=1");const ucl_object_t *a=ucl_object_lookup(o,"a");printf("priority:flags=%u,len=%u,ref=%u\n",a->flags,a->len,a->ref);ucl_object_unref(o);
}
static void conversions(void) {
    ucl_object_t *root=parse("i=42; neg=-1.75; f=1.75; t=2s; b=true; false=false; s=\"42\"; n=null; a=[1]; o={x=1}; zero=0; empty=\"\"; precise=123456.7890123; tiny=-0.0000001; whole=42.0; short=1ms");
    const char *keys[]={"i","neg","f","t","b","false","s","n","a","o","zero","empty","precise","tiny","whole","short","missing"};
    for(size_t j=0;j<sizeof(keys)/sizeof(*keys);j++) {
        const ucl_object_t *o=ucl_object_lookup(root,keys[j]);
        int64_t i=99;double d=99;bool b=true;const char *s="sentinel";size_t n=99;
        bool oi=ucl_object_toint_safe(o,&i),od=ucl_object_todouble_safe(o,&d),ob=ucl_object_toboolean_safe(o,&b);
        const char *before=s;bool os=ucl_object_tolstring_safe(o,&s,&n);
        printf("%s:type=%d,int=%d:%" PRId64 ",double=%d:%.17g,bool=%d:%d,str=%d:%zu,target-unchanged=%d\n",keys[j],ucl_object_type(o),oi,i,od,d,ob,b,os,n,s==before);
        printf("unsafe=%" PRId64 ",%.17g,%d\n",ucl_object_toint(o),ucl_object_todouble(o),ucl_object_toboolean(o));
        s="sentinel";before=s;os=ucl_object_tostring_safe(o,&s);printf("tostring-safe=%d,unchanged=%d\n",os,s==before);
        s=ucl_object_tostring(o);bytes("string",s,s?strlen(s):0);
        s=o?ucl_object_tostring_forced(o):NULL;bytes("forced",s,s?strlen(s):0);
    }
    for(int t=0;t<=8;t++)printf("type-name:%d=%s\n",t,ucl_object_type_to_string((ucl_type_t)t));
    const char *names[]={"integer","int","number","float","time","string","str","boolean","bool","object","array","null","userdata","INTEGER","unknown",""};
    for(size_t j=0;j<sizeof(names)/sizeof(*names);j++) {
        ucl_type_t result=UCL_NULL;bool ok=ucl_object_string_to_type(names[j],&result);printf("type-parse:%s=%d:%d\n",names[j],ok,result);
    }
    ucl_object_unref(root);
}
static void strings(void) {
    ucl_object_t *root=parse("s=\"a\\u0000b\"; \"k\\u0000x\"=7; unicode=\"é\"");
    const ucl_object_t *s=ucl_object_lookup(root,"s");size_t n=0;const char *text=ucl_object_tolstring(s,&n);bytes("nul-value",text,n);
    bytes("nul-zstring",ucl_object_tostring(s),strlen(ucl_object_tostring(s)));
    const char key[]={'k',0,'x'};const ucl_object_t *v=ucl_object_lookup_len(root,key,3);assert(v);
    printf("nul-key-value=%" PRId64 ",cstr-miss=%d\n",ucl_object_toint(v),ucl_object_lookup(root,"k")==NULL);
    text=ucl_object_keyl(v,&n);bytes("nul-key",text,n);bytes("nul-key-z",ucl_object_key(v),strlen(ucl_object_key(v)));
    s=ucl_object_lookup(root,"unicode");text=ucl_object_tolstring(s,&n);bytes("unicode",text,n);
    printf("bounded-key=%d,zero-keylen-miss=%d,wrong-type-lookup=%d,wrong-type-array-size=%u,missing-index=%d\n",ucl_object_lookup_len(root,"s!",1)==ucl_object_lookup(root,"s"),ucl_object_lookup_len(root,"s",0)==NULL,ucl_object_lookup(s,"x")==NULL,ucl_array_size(s),ucl_array_find_index(root,0)==NULL);
    ucl_object_unref(root);
}
static void iteration(void) {
    const char *inputs[]={"{}","[]","a=1","a=1;a=2;b=[3,4]","k={x=1};k={y=2}","k=[1];k=[2]","k=1;k={x=2};k=[3]","k={x=2};k=1;k=[3]","k=[3];k=1;k={x=2}","k=1;k=[3];k={x=2}","k=1;k=2;k=[3];k={x=4}"};
    for(size_t j=0;j<sizeof(inputs)/sizeof(*inputs);j++) {
        ucl_object_t *o=parse(inputs[j]);const ucl_object_t *targets[]={o,ucl_object_lookup(o,"k"),ucl_object_lookup(o,"a")};
        printf("document=%zu\n",j);
        for(size_t target=0;target<3;target++) {
            if(!targets[target])continue;
            for(int expand=0;expand<=1;expand++) {
                ucl_object_iter_t it=NULL;int err=123;const ucl_object_t *v;unsigned int count=0;
                printf("old:target=%zu,expand=%d\n",target,expand);
                while((v=ucl_object_iterate_with_error(targets[target],&it,expand,&err))) {
                    dump("next",v);printf("head=%d\n",v==targets[target]);assert(++count<20);
                }
                printf("count=%u,error=%d,cleared=%d\n",count,err,it==NULL);
                /* End is supported only for expanded object iterators. */
                if(expand && ucl_object_type(targets[target])==UCL_OBJECT)ucl_object_iterate_end(targets[target],&it);
            }
            for(int mode=1;mode<=3;mode++) {
                ucl_object_iter_t it=ucl_object_iterate_new(targets[target]);assert(it);
                const ucl_object_t *v;unsigned int count=0;printf("full:target=%zu,mode=%d\n",target,mode);
                while((v=ucl_object_iterate_full(it,(enum ucl_iterate_type)mode))) {dump("next",v);assert(++count<20);}
                printf("count=%u,exception=%d,exhausted-again=%d\n",count,ucl_object_iter_chk_excpn((ucl_object_iter_t *)it),ucl_object_iterate_full(it,(enum ucl_iterate_type)mode)==NULL);
                ucl_object_iterate_free(it);
            }
        }
        ucl_object_unref(o);
    }
    ucl_object_t *o=parse("a=1;a=2;b=[3,4]");
    ucl_object_iter_t it=NULL;const ucl_object_t *a=ucl_iterate_object(o,&it,true);printf("lookup-identity=%d\n",a==ucl_object_lookup(o,"a"));
    ucl_object_iterate_end(o,&it);printf("early-end-cleared=%d\n",it==NULL);
    it=ucl_object_iterate_new(o);const ucl_object_t *v;
    while((v=ucl_object_iterate_safe(it,true))) {size_t n=0;const char *k=ucl_object_keyl(v,&n);bytes("safe-key",k,n);}
    it=ucl_object_iterate_reset(it,a);
    while((v=ucl_object_iterate_safe(it,false)))dump("safe-implicit",v);
    ucl_object_iterate_free(it);ucl_object_unref(o);
}
static void lifetime(void) {
    struct ucl_parser *p=ucl_parser_new(0);assert(p && ucl_parser_add_string(p,"child={answer=42; quoted='x'}; a=1",0));
    ucl_object_t *root=ucl_parser_get_object(p);assert(root);printf("get-ref=%u\n",root->ref);
    ucl_object_t *again=ucl_parser_get_object(p);printf("again:same=%d,ref=%u\n",again==root,root->ref);ucl_object_unref(again);
    ucl_object_t *child=ucl_object_ref(ucl_object_lookup(root,"child"));ucl_object_t *a=ucl_object_ref(ucl_object_lookup(root,"a"));
    printf("child-ref=%u,scalar-ref=%u\n",child->ref,a->ref);ucl_parser_free(p);printf("root-after-parser=%u\n",root->ref);ucl_object_unref(root);
    printf("retained:child-ref=%u,scalar-ref=%u\n",child->ref,a->ref);dump("child-survives",child);
    for(int format=0;format<4;format++) {
        size_t length=0;unsigned char *out=ucl_object_emit_len(child,(ucl_emitter_t)format,&length);
        printf("retained-format=%d\n",format);bytes("retained-emit",(const char *)out,length);free(out);
    }
    printf("answer=%" PRId64 "\n",ucl_object_toint(ucl_object_lookup(child,"answer")));
    printf("retained-scalar-value=%" PRId64 "\n",ucl_object_toint(a));
    ucl_object_unref(child);ucl_object_unref(a);
}
static void emitters(void) {
    ucl_object_t *root=parse("a=1; a=2; child={q='quoted'; t=2s}; nul=\"a\\u0000b\"");
    const ucl_object_t *targets[]={root,ucl_object_lookup(root,"child"),ucl_object_lookup(root,"a"),ucl_object_lookup(root,"nul")};
    for(size_t j=0;j<sizeof(targets)/sizeof(*targets);j++)for(int f=0;f<4;f++) {
        size_t n=999;unsigned char *s=ucl_object_emit_len(targets[j],(enum ucl_emitter)f,&n);
        printf("target=%zu,format=%d,nul=%d\n",j,f,s && s[n]==0);bytes("emit",(const char *)s,s?n:0);
        unsigned char *other=ucl_object_emit(targets[j],(enum ucl_emitter)f);
        printf("same-bytes=%d\n",s && other && memcmp(s,other,n+1)==0);free(s);free(other);
    }
    size_t n=999;unsigned char *s=ucl_object_emit_len(NULL,UCL_EMIT_JSON_COMPACT,&n);printf("null-emit=%d,len=%zu\n",s==NULL,n);free(s);
    ucl_object_unref(root);
}
static void flags(void) {
    int flags[]={0,UCL_PARSER_KEY_LOWERCASE,UCL_PARSER_ZEROCOPY,UCL_PARSER_NO_TIME,UCL_PARSER_NO_IMPLICIT_ARRAYS,UCL_PARSER_SAVE_COMMENTS,UCL_PARSER_DISABLE_MACRO,UCL_PARSER_NO_FILEVARS};
    for(size_t j=0;j<sizeof(flags)/sizeof(*flags);j++) {
        struct ucl_parser *p=ucl_parser_new(flags[j]);assert(p);ucl_parser_register_variable(p,"V","expanded");
        bool ok=ucl_parser_add_string(p,"# comment\nA=1;A=2;a=3;t=2s;v=\"$V\"; filename=\"$FILENAME\"",0);
        printf("flag=%d,ok=%d\n",flags[j],ok);ucl_object_t *o=ucl_parser_get_object(p);dump("flags-root",o);ucl_object_unref(o);ucl_parser_free(p);
    }
}
static void filesystem(void) {
    FILE *f=fopen("fixture.ucl","w");assert(f);fputs("a=1\n",f);fclose(f);
    struct ucl_parser *p=ucl_parser_new(0);bool ok=ucl_parser_add_file(p,"fixture.ucl");diagnostic("file",p,ok);ucl_parser_free(p);
    p=ucl_parser_new(0);ok=ucl_parser_add_file(p,"missing.ucl");diagnostic("missing",p,ok);ucl_parser_free(p);
    p=ucl_parser_new(0);ok=ucl_parser_add_string(p,".include \"fixture.ucl\"",0);diagnostic("include",p,ok);ucl_parser_free(p);
    assert(mkdir("fixture-dir",0700)==0);
    f=fopen("fixture-dir/fixture.ucl","w");assert(f);fputs("a=99",f);fclose(f);
    f=fopen("fixture-dir/main.ucl","w");assert(f);fputs(".include \"fixture.ucl\"\nname=\"$FILENAME\"; dir=\"$CURDIR\"",f);fclose(f);
    p=ucl_parser_new(0);ok=ucl_parser_add_file(p,"fixture-dir/./main.ucl");assert(ok);
    ucl_object_t *o=ucl_parser_get_object(p);assert(o);
    char cwd[4096],expected[8192];assert(getcwd(cwd,sizeof(cwd)));
    snprintf(expected,sizeof(expected),"%s/fixture-dir/main.ucl",cwd);
    printf("file-include-cwd=%" PRId64 ",canonical-filename=%d,curdir-file-parent=%d\n",ucl_object_toint(ucl_object_lookup(o,"a")),strcmp(ucl_object_tostring(ucl_object_lookup(o,"name")),expected)==0,strncmp(ucl_object_tostring(ucl_object_lookup(o,"dir")),expected,strlen(expected)-strlen("/main.ucl"))==0 && strlen(ucl_object_tostring(ucl_object_lookup(o,"dir")))==strlen(expected)-strlen("/main.ucl"));
    ucl_object_unref(o);ucl_parser_free(p);
    remove("fixture-dir/main.ucl");remove("fixture-dir/fixture.ucl");rmdir("fixture-dir");remove("fixture.ucl");
}
int main(int argc,char **argv) {
    if(argc!=2)return 2;
#define CASE(n) if(strcmp(argv[1],#n)==0){n();return 0;}
    CASE(abi)CASE(parser)CASE(metadata)CASE(conversions)CASE(strings)CASE(iteration)CASE(lifetime)CASE(emitters)CASE(flags)CASE(filesystem)
    return 2;
}

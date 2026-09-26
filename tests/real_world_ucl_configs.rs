//! Real-world UCL configuration tests
//!
//! This module contains real-world UCL configurations from various projects
//! to test compatibility with actual usage patterns. Every input is also a conformance case
//! (`tests/conformance/cases/migrated/real_world_ucl_configs__real_world_configs__*`); the
//! expected results are libucl's.

use serde_json::{Value, json};
use serde_ucl::parse::ErrorKind;
use serde_ucl::{UclError, from_str};

#[cfg(test)]
mod real_world_configs {
    use super::*;

    #[test]
    fn test_nginx_real_config() {
        // Real NGINX configuration converted to UCL format
        let config = r#"
            user = "nginx";
            worker_processes = "auto";
            error_log = "/var/log/nginx/error.log";
            pid = "/run/nginx.pid";

            events {
                worker_connections = 1024;
                use = "epoll";
                multi_accept = true;
            }

            http {
                log_format = "main '$remote_addr - $remote_user [$time_local] \"$request\" '
                                  '$status $body_bytes_sent \"$http_referer\" '
                                  '\"$http_user_agent\" \"$http_x_forwarded_for\"'";

                access_log = "/var/log/nginx/access.log main";

                sendfile = true;
                tcp_nopush = true;
                tcp_nodelay = true;
                keepalive_timeout = 65;
                types_hash_max_size = 2048;

                include = "/etc/nginx/mime.types";
                default_type = "application/octet-stream";

                gzip = true;
                gzip_vary = true;
                gzip_proxied = "any";
                gzip_comp_level = 6;
                gzip_types = [
                    "text/plain",
                    "text/css",
                    "text/xml",
                    "text/javascript",
                    "application/json",
                    "application/javascript",
                    "application/xml+rss",
                    "application/atom+xml",
                    "image/svg+xml"
                ];

                upstream backend {
                    server = "127.0.0.1:8080 weight=3 max_fails=3 fail_timeout=30s";
                    server = "127.0.0.1:8081 weight=2 max_fails=3 fail_timeout=30s";
                    server = "127.0.0.1:8082 weight=1 backup";
                    
                    keepalive = 32;
                    keepalive_requests = 100;
                    keepalive_timeout = 60;
                }

                server {
                    listen = 80;
                    listen = "[::]:80";
                    server_name = "example.com www.example.com";
                    root = "/var/www/html";
                    index = "index.html index.htm index.nginx-debian.html";

                    location "/" {
                        try_files = "$uri $uri/ =404";
                    }

                    location "/api/" {
                        proxy_pass = "http://backend";
                        proxy_http_version = "1.1";
                        proxy_set_header = "Upgrade $http_upgrade";
                        proxy_set_header = "Connection 'upgrade'";
                        proxy_set_header = "Host $host";
                        proxy_set_header = "X-Real-IP $remote_addr";
                        proxy_set_header = "X-Forwarded-For $proxy_add_x_forwarded_for";
                        proxy_set_header = "X-Forwarded-Proto $scheme";
                        proxy_cache_bypass = "$http_upgrade";
                    }

                    location "~ \\.php$" {
                        include = "snippets/fastcgi-php.conf";
                        fastcgi_pass = "unix:/var/run/php/php7.4-fpm.sock";
                    }

                    location "~ /\\.ht" {
                        deny = "all";
                    }
                }
            }
        "#;

        // libucl rejects this input: its golden dump is an error (tests/conformance/cases/migrated/
        // real_world_ucl_configs__real_world_configs__test_nginx_real_config). The `log_format`
        // string holds raw line breaks, which double quotes do not allow (spec §6.1).
        match from_str::<Value>(config) {
            Err(UclError::Syntax(e)) => {
                assert_eq!(e.kind(), &ErrorKind::ControlCharacter { byte: b'\n' });
                assert_eq!(e.position().line, 14);
            }
            other => panic!("libucl rejects this input, got {other:?}"),
        }
    }

    #[test]
    fn test_freebsd_pkg_real_config() {
        // Real FreeBSD package configuration
        let config = r#"
            name = "nginx";
            version = "1.20.1";
            origin = "www/nginx";
            comment = "Robust and small WWW server";
            desc = <<EOD
Nginx (pronounced "engine x") is a free, open-source, high-performance HTTP
server and reverse proxy, as well as an IMAP/POP3 proxy server. Igor Sysoev
started development of Nginx in 2002, with the first public release in 2004.
Nginx now hosts nearly 12.6% (22.2M) of active sites across all domains.

Nginx is known for its high performance, stability, rich feature set, simple
configuration, and low resource consumption.
EOD

            maintainer = "demon@FreeBSD.org";
            www = "https://nginx.org/";

            arch = "FreeBSD:13:amd64";
            prefix = "/usr/local";

            deps {
                pcre {
                    origin = "devel/pcre";
                    version = "8.45";
                }
                openssl {
                    origin = "security/openssl";
                    version = "1.1.1k,1";
                }
                zlib {
                    origin = "archivers/zlib";
                    version = "1.2.11";
                }
            }

            files {
                "/usr/local/sbin/nginx" = "sha256:a1b2c3d4e5f6...";
                "/usr/local/etc/nginx/nginx.conf" = "sha256:f6e5d4c3b2a1...";
                "/usr/local/etc/nginx/mime.types" = "sha256:1a2b3c4d5e6f...";
                "/usr/local/www/nginx/index.html" = "sha256:6f5e4d3c2b1a...";
            }

            directories {
                "/usr/local/etc/nginx" = true;
                "/usr/local/www/nginx" = true;
                "/var/log/nginx" = true;
                "/var/run/nginx" = true;
            }

            scripts {
                "pre-install" = <<SCRIPT
#!/bin/sh
if ! pw groupshow www >/dev/null 2>&1; then
    pw groupadd www -g 80
fi
if ! pw usershow www >/dev/null 2>&1; then
    pw useradd www -u 80 -g www -d /nonexistent -s /usr/sbin/nologin
fi
SCRIPT

                "post-install" = <<SCRIPT
#!/bin/sh
echo "Nginx has been installed successfully."
echo "Configuration files are in /usr/local/etc/nginx/"
echo "To start nginx: service nginx start"
SCRIPT
            }

            options {
                "HTTP_SSL" = true;
                "HTTP_GZIP" = true;
                "HTTP_REWRITE" = true;
                "HTTP_REALIP" = true;
                "HTTP_STATUS" = false;
                "HTTP_DAV" = false;
                "MAIL" = false;
                "STREAM" = false;
            }

            categories = ["www", "http"];
            licenses = ["BSD2CLAUSE"];

            annotations {
                "repo_type" = "binary";
                "built_by" = "poudriere-devel-3.3.0";
                "build_timestamp" = "1625097600";
            }
        "#;

        let parsed: Value = from_str(config).expect("FreeBSD pkg config parses");
        let obj = parsed.as_object().unwrap();

        assert_eq!(obj["name"], "nginx");
        assert_eq!(obj["version"], "1.20.1");
        assert_eq!(obj["origin"], "www/nginx");
        assert_eq!(obj["maintainer"], "demon@FreeBSD.org");

        // Heredocs (spec §6.3): the content lines, without the line break before the terminator.
        let desc = obj["desc"].as_str().unwrap();
        assert!(desc.starts_with("Nginx (pronounced \"engine x\") is a free"));
        assert!(desc.ends_with("configuration, and low resource consumption."));
        assert_eq!(
            obj["scripts"]["post-install"],
            "#!/bin/sh\necho \"Nginx has been installed successfully.\"\n\
             echo \"Configuration files are in /usr/local/etc/nginx/\"\n\
             echo \"To start nginx: service nginx start\""
        );
        assert!(
            obj["scripts"]["pre-install"]
                .as_str()
                .unwrap()
                .starts_with("#!/bin/sh\nif ! pw groupshow www")
        );

        // Nested structures
        assert_eq!(obj["deps"]["openssl"]["version"], "1.1.1k,1");
        assert_eq!(obj["files"].as_object().unwrap().len(), 4);
        assert_eq!(obj["directories"]["/var/log/nginx"], true);
        assert_eq!(obj["options"]["HTTP_SSL"], true);
        assert_eq!(obj["options"]["HTTP_STATUS"], false);
        // A quoted number stays a string (spec §6).
        assert_eq!(obj["annotations"]["build_timestamp"], "1625097600");

        // Arrays
        assert_eq!(obj["categories"], json!(["www", "http"]));
        assert_eq!(obj["licenses"], json!(["BSD2CLAUSE"]));
    }

    #[test]
    fn test_rspamd_real_config() {
        // Real Rspamd configuration
        let config = r#"
            # Rspamd main configuration
            
            logging {
                type = "file";
                filename = "/var/log/rspamd/rspamd.log";
                level = "info";
                log_buffer = 32768;
                log_urls = false;
            }

            options {
                pidfile = "/var/run/rspamd/rspamd.pid";
                filters = ["chartable", "dkim", "spf", "surbl", "regexp"];
                raw_mode = false;
                one_shot = false;
                cache_file = "/var/lib/rspamd/symbols.cache";
                map_watch_interval = 60;
                dynamic_conf = "/var/lib/rspamd/rspamd_dynamic";
                history_file = "/var/lib/rspamd/rspamd.history";
                check_all_filters = false;
                dns {
                    timeout = 1;
                    sockets = 16;
                    retransmits = 5;
                    nameserver = ["8.8.8.8", "1.1.1.1"];
                }
            }

            worker "normal" {
                bind_socket = "localhost:11333";
                count = 1;
                max_tasks = 1000;
                task_timeout = 8;
                keypair {
                    pubkey = "ob6kwq45w9pbs3s4hrbe1ky4w3r6o6t1xbixbqx6r4dxngz5m6c1y";
                    privkey = "ed25519:private_key_here";
                }
            }

            worker "controller" {
                bind_socket = "localhost:11334";
                count = 1;
                secure_ip = ["127.0.0.1", "::1"];
                password = "$2$rounds=12000$salt$hash";
                enable_password = "$2$rounds=12000$salt$hash";
                static_dir = "/usr/share/rspamd/www/";
                stats_path = "/var/lib/rspamd/stats.ucl";
            }

            worker "rspamd_proxy" {
                bind_socket = "localhost:11332";
                milter = true;
                timeout = 120;
                upstream "local" {
                    default = true;
                    hosts = "localhost:11333";
                }
            }

            modules {
                path = "/usr/share/rspamd/lib/";
            }

            lua = "/etc/rspamd/rspamd.lua";

            metric "default" {
                actions {
                    reject = 15;
                    add_header = 6;
                    greylist = 4;
                    "soft reject" = 10;
                }
                
                unknown_weight = 1;
                subject = "***SPAM*** %s";
                
                group "header" {
                    weight = 1;
                    description = "Header-based checks";
                }
                
                group "content" {
                    weight = 1;
                    description = "Content-based checks";
                }
            }

            classifier "bayes" {
                tokenizer {
                    name = "osb-text";
                }
                
                cache {
                    path = "/var/lib/rspamd/learn_cache.sqlite";
                }
                
                min_learns = 200;
                backend = "sqlite3";
                languages_enabled = true;
                
                statfile {
                    symbol = "BAYES_HAM";
                    path = "/var/lib/rspamd/bayes.ham.sqlite";
                    spam = false;
                }
                
                statfile {
                    symbol = "BAYES_SPAM";
                    path = "/var/lib/rspamd/bayes.spam.sqlite";
                    spam = true;
                }
            }

            composites {
                "FORGED_RECIPIENTS" = "FORGED_RECIPIENTS_MAILRU | FORGED_RECIPIENTS_GMAIL";
                "SUSPICIOUS_RECIPS" = "SUSPICIOUS_RECIPS & !WHITELIST_SPF";
                "DKIM_MIXED" = "R_DKIM_ALLOW & R_DKIM_REJECT";
            }
        "#;

        let parsed: Value = from_str(config).expect("Rspamd config parses");
        let obj = parsed.as_object().unwrap();

        assert_eq!(obj["logging"]["log_buffer"], 32768);
        assert_eq!(obj["logging"]["log_urls"], false);
        assert_eq!(obj["options"]["filters"].as_array().unwrap().len(), 5);
        assert_eq!(
            obj["options"]["dns"]["nameserver"],
            json!(["8.8.8.8", "1.1.1.1"])
        );
        assert_eq!(obj["modules"]["path"], "/usr/share/rspamd/lib/");
        assert_eq!(obj["lua"], "/etc/rspamd/rspamd.lua");

        // `worker "normal" { … }` is the object `worker { normal { … } }`, and the three
        // `worker` sections are three values of one key (spec §3.4, §8.2).
        let workers = obj["worker"].as_array().unwrap();
        assert_eq!(workers.len(), 3);
        assert_eq!(workers[0]["normal"]["bind_socket"], "localhost:11333");
        assert_eq!(
            workers[1]["controller"]["secure_ip"],
            json!(["127.0.0.1", "::1"])
        );
        // `$2` and the other references match no registered variable, so they stay (spec §7.4).
        assert_eq!(
            workers[1]["controller"]["password"],
            "$2$rounds=12000$salt$hash"
        );
        assert_eq!(
            workers[2]["rspamd_proxy"]["upstream"]["local"]["default"],
            true
        );

        let metric = &obj["metric"]["default"];
        assert_eq!(metric["actions"]["soft reject"], 10);
        assert_eq!(metric["subject"], "***SPAM*** %s");
        assert_eq!(
            metric["group"],
            json!([
                {"header": {"weight": 1, "description": "Header-based checks"}},
                {"content": {"weight": 1, "description": "Content-based checks"}}
            ])
        );

        let bayes = &obj["classifier"]["bayes"];
        assert_eq!(bayes["min_learns"], 200);
        let statfiles = bayes["statfile"].as_array().unwrap();
        assert_eq!(statfiles.len(), 2);
        assert_eq!(statfiles[1]["symbol"], "BAYES_SPAM");

        assert_eq!(
            obj["composites"]["SUSPICIOUS_RECIPS"],
            "SUSPICIOUS_RECIPS & !WHITELIST_SPF"
        );
    }

    #[test]
    fn test_dovecot_ucl_config() {
        // Dovecot-style configuration in UCL format
        let config = r#"
            # Dovecot configuration in UCL format
            
            protocols = ["imap", "pop3", "lmtp"];
            
            listen = ["*", "::"];
            base_dir = "/var/run/dovecot/";
            instance_name = "dovecot";
            
            login_greeting = "Dovecot ready.";
            login_trusted_networks = ["127.0.0.0/8", "10.0.0.0/8"];
            
            mail_location = "maildir:~/Maildir";
            mail_uid = "vmail";
            mail_gid = "vmail";
            
            namespace "inbox" {
                type = "private";
                separator = "/";
                prefix = "";
                location = "";
                inbox = true;
                hidden = false;
                list = true;
                subscriptions = true;
                
                mailbox "Drafts" {
                    special_use = "\\Drafts";
                }
                
                mailbox "Junk" {
                    special_use = "\\Junk";
                }
                
                mailbox "Trash" {
                    special_use = "\\Trash";
                }
                
                mailbox "Sent" {
                    special_use = "\\Sent";
                }
            }
            
            service "imap-login" {
                inet_listener "imap" {
                    port = 143;
                }
                
                inet_listener "imaps" {
                    port = 993;
                    ssl = true;
                }
                
                process_min_avail = 0;
                process_limit = 1000;
            }
            
            service "pop3-login" {
                inet_listener "pop3" {
                    port = 110;
                }
                
                inet_listener "pop3s" {
                    port = 995;
                    ssl = true;
                }
            }
            
            service "lmtp" {
                unix_listener "lmtp" {
                    path = "/var/spool/postfix/private/dovecot-lmtp";
                    mode = "0600";
                    user = "postfix";
                    group = "postfix";
                }
            }
            
            service "auth" {
                unix_listener "auth-userdb" {
                    path = "/var/spool/postfix/private/auth";
                    mode = "0666";
                    user = "postfix";
                    group = "postfix";
                }
                
                unix_listener "/var/run/dovecot/auth-master" {
                    mode = "0600";
                    user = "vmail";
                }
                
                user = "$default_internal_user";
            }
            
            auth_mechanisms = ["plain", "login"];
            
            passdb {
                driver = "pam";
            }
            
            userdb {
                driver = "passwd";
            }
            
            ssl_cert = "</etc/ssl/certs/dovecot.pem";
            ssl_key = "</etc/ssl/private/dovecot.pem";
            ssl_protocols = ["!SSLv2", "!SSLv3"];
            ssl_cipher_list = "ECDHE+AESGCM:DH+AESGCM:ECDHE+AES:DH+AES:RSA+AESGCM:RSA+AES:!aNULL:!MD5:!DSS";
            ssl_prefer_server_ciphers = true;
            ssl_dh_parameters_length = 2048;
        "#;

        let parsed: Value = from_str(config).expect("Dovecot config parses");
        let obj = parsed.as_object().unwrap();

        // Arrays
        assert_eq!(obj["protocols"], json!(["imap", "pop3", "lmtp"]));
        assert_eq!(obj["listen"], json!(["*", "::"]));
        assert_eq!(obj["login_trusted_networks"].as_array().unwrap().len(), 2);
        assert_eq!(obj["auth_mechanisms"], json!(["plain", "login"]));
        assert_eq!(obj["ssl_protocols"], json!(["!SSLv2", "!SSLv3"]));

        // Strings
        assert_eq!(obj["base_dir"], "/var/run/dovecot/");
        assert_eq!(obj["mail_location"], "maildir:~/Maildir");
        assert_eq!(obj["mail_uid"], "vmail");
        assert_eq!(obj["ssl_cert"], "</etc/ssl/certs/dovecot.pem");
        assert_eq!(obj["ssl_dh_parameters_length"], 2048);

        // Named sections
        let inbox = &obj["namespace"]["inbox"];
        assert_eq!(inbox["separator"], "/");
        assert_eq!(inbox["prefix"], "");
        assert_eq!(inbox["mailbox"].as_array().unwrap().len(), 4);
        assert_eq!(inbox["mailbox"][0]["Drafts"]["special_use"], "\\Drafts");

        let services = obj["service"].as_array().unwrap();
        assert_eq!(services.len(), 4);
        assert_eq!(services[0]["imap-login"]["process_limit"], 1000);
        assert_eq!(
            services[0]["imap-login"]["inet_listener"][1]["imaps"]["port"],
            993
        );
        assert_eq!(services[3]["auth"]["user"], "$default_internal_user");
    }

    #[test]
    fn test_haproxy_ucl_config() {
        // HAProxy configuration in UCL format
        let config = r#"
            global {
                daemon = true;
                chroot = "/var/lib/haproxy";
                stats = "socket /run/haproxy/admin.sock mode 660 level admin";
                stats = "timeout 30s";
                user = "haproxy";
                group = "haproxy";
                
                # Default SSL material locations
                ca_base = "/etc/ssl/certs";
                crt_base = "/etc/ssl/private";
                
                # Default ciphers to use on SSL-enabled listening sockets
                ssl_default_bind_ciphers = "ECDH+AESGCM:DH+AESGCM:ECDH+AES256:DH+AES256:ECDH+AES128:DH+AES:RSA+AESGCM:RSA+AES:!aNULL:!MD5:!DSS";
                ssl_default_bind_options = "no-sslv3";
            }
            
            defaults {
                log = "global";
                mode = "http";
                option = "httplog";
                option = "dontlognull";
                timeout = "connect 5000";
                timeout = "client  50000";
                timeout = "server  50000";
                errorfile = "400 /etc/haproxy/errors/400.http";
                errorfile = "403 /etc/haproxy/errors/403.http";
                errorfile = "408 /etc/haproxy/errors/408.http";
                errorfile = "500 /etc/haproxy/errors/500.http";
                errorfile = "502 /etc/haproxy/errors/502.http";
                errorfile = "503 /etc/haproxy/errors/503.http";
                errorfile = "504 /etc/haproxy/errors/504.http";
            }
            
            frontend "web_frontend" {
                bind = "*:80";
                bind = "*:443 ssl crt /etc/ssl/certs/example.com.pem";
                redirect = "scheme https if !{ ssl_fc }";
                default_backend = "web_servers";
                
                # ACLs for routing
                acl = "is_api path_beg /api/";
                acl = "is_static path_beg /static/";
                
                use_backend = "api_servers if is_api";
                use_backend = "static_servers if is_static";
            }
            
            backend "web_servers" {
                balance = "roundrobin";
                option = "httpchk GET /health";
                
                server = "web1 192.168.1.10:8080 check";
                server = "web2 192.168.1.11:8080 check";
                server = "web3 192.168.1.12:8080 check backup";
            }
            
            backend "api_servers" {
                balance = "leastconn";
                option = "httpchk GET /api/health";
                
                server = "api1 192.168.1.20:3000 check";
                server = "api2 192.168.1.21:3000 check";
            }
            
            backend "static_servers" {
                balance = "source";
                
                server = "static1 192.168.1.30:80 check";
                server = "static2 192.168.1.31:80 check";
            }
            
            listen "stats" {
                bind = "*:8404";
                stats = "enable";
                stats = "uri /stats";
                stats = "refresh 30s";
                stats = "admin if TRUE";
            }
        "#;

        let parsed: Value = from_str(config).expect("HAProxy config parses");
        let obj = parsed.as_object().unwrap();

        // Repeated keys hold several values (spec §8.2).
        assert_eq!(obj["global"]["daemon"], true);
        assert_eq!(
            obj["global"]["stats"],
            json!([
                "socket /run/haproxy/admin.sock mode 660 level admin",
                "timeout 30s"
            ])
        );
        assert_eq!(obj["defaults"]["option"], json!(["httplog", "dontlognull"]));
        assert_eq!(
            obj["defaults"]["timeout"],
            json!(["connect 5000", "client  50000", "server  50000"])
        );
        assert_eq!(obj["defaults"]["errorfile"].as_array().unwrap().len(), 7);

        // Named sections
        let frontend = &obj["frontend"]["web_frontend"];
        assert_eq!(frontend["redirect"], "scheme https if !{ ssl_fc }");
        assert_eq!(frontend["acl"].as_array().unwrap().len(), 2);
        let backends = obj["backend"].as_array().unwrap();
        assert_eq!(backends.len(), 3);
        assert_eq!(backends[1]["api_servers"]["balance"], "leastconn");
        assert_eq!(
            backends[2]["static_servers"]["server"],
            json!([
                "static1 192.168.1.30:80 check",
                "static2 192.168.1.31:80 check"
            ])
        );
        assert_eq!(obj["listen"]["stats"]["stats"].as_array().unwrap().len(), 4);
    }
}

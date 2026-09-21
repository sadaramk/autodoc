//! Knowledge of well-known libraries and images: which ones mean "this unit
//! talks to Postgres" or "this unit is an HTTP service". Matching is exact on
//! package names (or prefix for Go module paths), never fuzzy.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum InfraCategory {
    Storage,
    EventBus,
    ThirdParty,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum InfraKind {
    Postgres,
    Mysql,
    Sqlite,
    Mongodb,
    Redis,
    Elasticsearch,
    S3,
    Firestore,
    Dynamodb,
    Kafka,
    Nats,
    Rabbitmq,
    Sqs,
    Pubsub,
    Stripe,
    Sendgrid,
    Twilio,
    Openai,
    Anthropic,
    Slack,
    Smtp,
}

impl InfraKind {
    pub fn id(self) -> &'static str {
        match self {
            InfraKind::Postgres => "postgres",
            InfraKind::Mysql => "mysql",
            InfraKind::Sqlite => "sqlite",
            InfraKind::Mongodb => "mongodb",
            InfraKind::Redis => "redis",
            InfraKind::Elasticsearch => "elasticsearch",
            InfraKind::S3 => "s3",
            InfraKind::Firestore => "firestore",
            InfraKind::Dynamodb => "dynamodb",
            InfraKind::Pubsub => "pubsub",
            InfraKind::Kafka => "kafka",
            InfraKind::Nats => "nats",
            InfraKind::Rabbitmq => "rabbitmq",
            InfraKind::Sqs => "sqs",
            InfraKind::Stripe => "stripe",
            InfraKind::Sendgrid => "sendgrid",
            InfraKind::Twilio => "twilio",
            InfraKind::Openai => "openai",
            InfraKind::Anthropic => "anthropic",
            InfraKind::Slack => "slack",
            InfraKind::Smtp => "smtp",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            InfraKind::Postgres => "PostgreSQL",
            InfraKind::Mysql => "MySQL",
            InfraKind::Sqlite => "SQLite",
            InfraKind::Mongodb => "MongoDB",
            InfraKind::Redis => "Redis",
            InfraKind::Elasticsearch => "Elasticsearch",
            InfraKind::S3 => "Object storage",
            InfraKind::Firestore => "Firestore",
            InfraKind::Dynamodb => "DynamoDB",
            InfraKind::Pubsub => "Pub/Sub",
            InfraKind::Kafka => "Kafka",
            InfraKind::Nats => "NATS",
            InfraKind::Rabbitmq => "RabbitMQ",
            InfraKind::Sqs => "SQS",
            InfraKind::Stripe => "Stripe",
            InfraKind::Sendgrid => "SendGrid",
            InfraKind::Twilio => "Twilio",
            InfraKind::Openai => "OpenAI",
            InfraKind::Anthropic => "Anthropic",
            InfraKind::Slack => "Slack",
            InfraKind::Smtp => "Email (SMTP)",
        }
    }

    pub fn role(self) -> &'static str {
        match self {
            InfraKind::Postgres | InfraKind::Mysql | InfraKind::Sqlite => "Relational database",
            InfraKind::Mongodb => "Document store",
            InfraKind::Redis => "Cache",
            InfraKind::Elasticsearch => "Search index",
            InfraKind::S3 => "Blob storage",
            InfraKind::Firestore | InfraKind::Dynamodb => "Document store",
            InfraKind::Pubsub => "Event bus",
            InfraKind::Kafka | InfraKind::Nats | InfraKind::Rabbitmq | InfraKind::Sqs => "Event bus",
            InfraKind::Stripe => "Payments API",
            InfraKind::Sendgrid => "Email delivery",
            InfraKind::Twilio => "SMS & voice",
            InfraKind::Openai | InfraKind::Anthropic => "LLM API",
            InfraKind::Slack => "Chat API",
            InfraKind::Smtp => "Email delivery",
        }
    }

    pub fn category(self) -> InfraCategory {
        match self {
            InfraKind::Kafka | InfraKind::Nats | InfraKind::Rabbitmq | InfraKind::Sqs | InfraKind::Pubsub => {
                InfraCategory::EventBus
            }
            InfraKind::Stripe
            | InfraKind::Sendgrid
            | InfraKind::Twilio
            | InfraKind::Openai
            | InfraKind::Anthropic
            | InfraKind::Slack
            | InfraKind::Smtp => InfraCategory::ThirdParty,
            _ => InfraCategory::Storage,
        }
    }

    pub fn is_sql(self) -> bool {
        matches!(self, InfraKind::Postgres | InfraKind::Mysql | InfraKind::Sqlite)
    }
}

/// (package name or Go module prefix, kind)
const LIBRARIES: &[(&str, InfraKind)] = &[
    // NuGet. `Microsoft.EntityFrameworkCore` alone names no store; the provider does.
    ("Npgsql.EntityFrameworkCore.PostgreSQL", InfraKind::Postgres),
    ("Microsoft.EntityFrameworkCore.Npgsql", InfraKind::Postgres),
    ("Npgsql", InfraKind::Postgres),
    ("Pomelo.EntityFrameworkCore.MySql", InfraKind::Mysql),
    ("MySqlConnector", InfraKind::Mysql),
    ("Microsoft.EntityFrameworkCore.Sqlite", InfraKind::Sqlite),
    ("StackExchange.Redis", InfraKind::Redis),
    ("Microsoft.Extensions.Caching.StackExchangeRedis", InfraKind::Redis),
    ("MongoDB.Driver", InfraKind::Mongodb),
    ("AWSSDK.S3", InfraKind::S3),
    ("AWSSDK.DynamoDBv2", InfraKind::Dynamodb),
    ("AWSSDK.SQS", InfraKind::Sqs),
    ("Confluent.Kafka", InfraKind::Kafka),
    ("RabbitMQ.Client", InfraKind::Rabbitmq),
    ("NATS.Client.Core", InfraKind::Nats),
    ("Elastic.Clients.Elasticsearch", InfraKind::Elasticsearch),
    ("NEST", InfraKind::Elasticsearch),
    ("Stripe.net", InfraKind::Stripe),
    ("SendGrid", InfraKind::Sendgrid),
    ("Twilio", InfraKind::Twilio),
    ("OpenAI", InfraKind::Openai),
    ("Anthropic.SDK", InfraKind::Anthropic),
    ("MailKit", InfraKind::Smtp),
    // Postgres
    ("pg", InfraKind::Postgres),
    ("postgres", InfraKind::Postgres),
    ("pg-promise", InfraKind::Postgres),
    ("@prisma/client", InfraKind::Postgres),
    ("sqlx", InfraKind::Postgres),
    ("tokio-postgres", InfraKind::Postgres),
    ("diesel", InfraKind::Postgres),
    ("psycopg", InfraKind::Postgres),
    ("psycopg2", InfraKind::Postgres),
    ("psycopg2-binary", InfraKind::Postgres),
    ("asyncpg", InfraKind::Postgres),
    ("github.com/jackc/pgx", InfraKind::Postgres),
    ("github.com/lib/pq", InfraKind::Postgres),
    // MySQL / SQLite / Mongo
    ("mysql2", InfraKind::Mysql),
    ("mysqlclient", InfraKind::Mysql),
    ("pymysql", InfraKind::Mysql),
    ("github.com/go-sql-driver/mysql", InfraKind::Mysql),
    ("better-sqlite3", InfraKind::Sqlite),
    ("sqlite3", InfraKind::Sqlite),
    ("rusqlite", InfraKind::Sqlite),
    ("github.com/mattn/go-sqlite3", InfraKind::Sqlite),
    ("mongodb", InfraKind::Mongodb),
    ("mongoose", InfraKind::Mongodb),
    ("pymongo", InfraKind::Mongodb),
    ("go.mongodb.org/mongo-driver", InfraKind::Mongodb),
    // Redis
    ("redis", InfraKind::Redis),
    ("ioredis", InfraKind::Redis),
    ("fred", InfraKind::Redis),
    ("github.com/redis/go-redis", InfraKind::Redis),
    ("github.com/go-redis/redis", InfraKind::Redis),
    ("@elastic/elasticsearch", InfraKind::Elasticsearch),
    ("elasticsearch", InfraKind::Elasticsearch),
    ("@aws-sdk/client-s3", InfraKind::S3),
    ("aws-sdk-s3", InfraKind::S3),
    ("boto3", InfraKind::S3),
    // Messaging
    ("kafkajs", InfraKind::Kafka),
    ("rdkafka", InfraKind::Kafka),
    ("confluent-kafka", InfraKind::Kafka),
    ("kafka-python", InfraKind::Kafka),
    ("aiokafka", InfraKind::Kafka),
    ("github.com/segmentio/kafka-go", InfraKind::Kafka),
    ("github.com/confluentinc/confluent-kafka-go", InfraKind::Kafka),
    ("github.com/IBM/sarama", InfraKind::Kafka),
    ("nats", InfraKind::Nats),
    ("async-nats", InfraKind::Nats),
    ("nats-py", InfraKind::Nats),
    ("github.com/nats-io/nats.go", InfraKind::Nats),
    ("amqplib", InfraKind::Rabbitmq),
    ("lapin", InfraKind::Rabbitmq),
    ("pika", InfraKind::Rabbitmq),
    ("github.com/rabbitmq/amqp091-go", InfraKind::Rabbitmq),
    ("@aws-sdk/client-sqs", InfraKind::Sqs),
    ("cloud.google.com/go/firestore", InfraKind::Firestore),
    ("@google-cloud/firestore", InfraKind::Firestore),
    ("firebase-admin", InfraKind::Firestore),
    ("google-cloud-firestore", InfraKind::Firestore),
    ("@aws-sdk/client-dynamodb", InfraKind::Dynamodb),
    ("aws-sdk-dynamodb", InfraKind::Dynamodb),
    ("github.com/aws/aws-sdk-go-v2/service/dynamodb", InfraKind::Dynamodb),
    ("cloud.google.com/go/pubsub", InfraKind::Pubsub),
    ("@google-cloud/pubsub", InfraKind::Pubsub),
    ("google-cloud-pubsub", InfraKind::Pubsub),
    ("aws-sdk-sqs", InfraKind::Sqs),
    // Third parties
    ("stripe", InfraKind::Stripe),
    ("async-stripe", InfraKind::Stripe),
    ("github.com/stripe/stripe-go", InfraKind::Stripe),
    ("@sendgrid/mail", InfraKind::Sendgrid),
    ("sendgrid", InfraKind::Sendgrid),
    ("github.com/sendgrid/sendgrid-go", InfraKind::Sendgrid),
    ("twilio", InfraKind::Twilio),
    ("github.com/twilio/twilio-go", InfraKind::Twilio),
    ("openai", InfraKind::Openai),
    ("async-openai", InfraKind::Openai),
    ("github.com/sashabaranov/go-openai", InfraKind::Openai),
    ("@anthropic-ai/sdk", InfraKind::Anthropic),
    ("anthropic", InfraKind::Anthropic),
    ("github.com/anthropics/anthropic-sdk-go", InfraKind::Anthropic),
    ("@slack/web-api", InfraKind::Slack),
    ("slack-sdk", InfraKind::Slack),
    // Generic SMTP (vendor SDKs above keep their own identity)
    ("nodemailer", InfraKind::Smtp),
    ("emailjs", InfraKind::Smtp),
    ("smtplib", InfraKind::Smtp),
    ("aiosmtplib", InfraKind::Smtp),
    ("fastapi-mail", InfraKind::Smtp),
    ("flask-mail", InfraKind::Smtp),
    ("lettre", InfraKind::Smtp),
    ("net/smtp", InfraKind::Smtp),
    ("gopkg.in/gomail.v2", InfraKind::Smtp),
    ("github.com/go-gomail/gomail", InfraKind::Smtp),
    // JVM (Maven/Gradle `group:artifact`)
    ("org.postgresql:postgresql", InfraKind::Postgres),
    ("org.postgresql:r2dbc-postgresql", InfraKind::Postgres),
    ("io.r2dbc:r2dbc-postgresql", InfraKind::Postgres),
    ("io.vertx:vertx-pg-client", InfraKind::Postgres),
    ("io.quarkus:quarkus-jdbc-postgresql", InfraKind::Postgres),
    ("io.quarkus:quarkus-reactive-pg-client", InfraKind::Postgres),
    ("mysql:mysql-connector-java", InfraKind::Mysql),
    ("com.mysql:mysql-connector-j", InfraKind::Mysql),
    ("org.mariadb.jdbc:mariadb-java-client", InfraKind::Mysql),
    ("io.quarkus:quarkus-jdbc-mysql", InfraKind::Mysql),
    ("io.quarkus:quarkus-jdbc-mariadb", InfraKind::Mysql),
    ("org.xerial:sqlite-jdbc", InfraKind::Sqlite),
    ("org.springframework.boot:spring-boot-starter-data-mongodb", InfraKind::Mongodb),
    ("org.springframework.boot:spring-boot-starter-data-mongodb-reactive", InfraKind::Mongodb),
    ("org.mongodb:mongodb-driver-sync", InfraKind::Mongodb),
    ("org.mongodb:mongodb-driver-reactivestreams", InfraKind::Mongodb),
    ("org.mongodb:mongo-java-driver", InfraKind::Mongodb),
    ("io.quarkus:quarkus-mongodb-client", InfraKind::Mongodb),
    ("io.quarkus:quarkus-mongodb-panache", InfraKind::Mongodb),
    ("io.micronaut.mongodb:micronaut-mongo-sync", InfraKind::Mongodb),
    ("org.springframework.boot:spring-boot-starter-data-redis", InfraKind::Redis),
    ("org.springframework.boot:spring-boot-starter-data-redis-reactive", InfraKind::Redis),
    ("redis.clients:jedis", InfraKind::Redis),
    ("io.lettuce:lettuce-core", InfraKind::Redis),
    ("org.redisson:redisson", InfraKind::Redis),
    ("io.quarkus:quarkus-redis-client", InfraKind::Redis),
    ("org.springframework.boot:spring-boot-starter-data-elasticsearch", InfraKind::Elasticsearch),
    ("co.elastic.clients:elasticsearch-java", InfraKind::Elasticsearch),
    ("org.elasticsearch.client:elasticsearch-rest-high-level-client", InfraKind::Elasticsearch),
    ("org.opensearch.client:opensearch-java", InfraKind::Elasticsearch),
    ("software.amazon.awssdk:s3", InfraKind::S3),
    ("com.amazonaws:aws-java-sdk-s3", InfraKind::S3),
    ("io.minio:minio", InfraKind::S3),
    ("software.amazon.awssdk:dynamodb", InfraKind::Dynamodb),
    ("software.amazon.awssdk:dynamodb-enhanced", InfraKind::Dynamodb),
    ("com.google.cloud:google-cloud-firestore", InfraKind::Firestore),
    ("com.google.cloud:google-cloud-pubsub", InfraKind::Pubsub),
    ("org.springframework.kafka:spring-kafka", InfraKind::Kafka),
    ("org.apache.kafka:kafka-clients", InfraKind::Kafka),
    ("org.apache.kafka:kafka-streams", InfraKind::Kafka),
    ("org.springframework.cloud:spring-cloud-starter-stream-kafka", InfraKind::Kafka),
    ("org.springframework.cloud:spring-cloud-stream-binder-kafka", InfraKind::Kafka),
    ("io.quarkus:quarkus-smallrye-reactive-messaging-kafka", InfraKind::Kafka),
    ("io.quarkus:quarkus-messaging-kafka", InfraKind::Kafka),
    ("io.quarkus:quarkus-kafka-client", InfraKind::Kafka),
    ("io.micronaut.kafka:micronaut-kafka", InfraKind::Kafka),
    ("org.springframework.boot:spring-boot-starter-amqp", InfraKind::Rabbitmq),
    ("org.springframework.amqp:spring-rabbit", InfraKind::Rabbitmq),
    ("com.rabbitmq:amqp-client", InfraKind::Rabbitmq),
    ("org.springframework.cloud:spring-cloud-starter-stream-rabbit", InfraKind::Rabbitmq),
    ("org.springframework.cloud:spring-cloud-starter-bus-amqp", InfraKind::Rabbitmq),
    ("io.quarkus:quarkus-smallrye-reactive-messaging-rabbitmq", InfraKind::Rabbitmq),
    ("io.quarkus:quarkus-messaging-rabbitmq", InfraKind::Rabbitmq),
    ("io.micronaut.rabbitmq:micronaut-rabbitmq", InfraKind::Rabbitmq),
    ("software.amazon.awssdk:sqs", InfraKind::Sqs),
    ("io.awspring.cloud:spring-cloud-aws-starter-sqs", InfraKind::Sqs),
    ("io.nats:jnats", InfraKind::Nats),
    ("com.stripe:stripe-java", InfraKind::Stripe),
    ("com.sendgrid:sendgrid-java", InfraKind::Sendgrid),
    ("com.twilio.sdk:twilio", InfraKind::Twilio),
    ("com.slack.api:slack-api-client", InfraKind::Slack),
    ("org.springframework.boot:spring-boot-starter-mail", InfraKind::Smtp),
    ("com.sun.mail:jakarta.mail", InfraKind::Smtp),
    ("jakarta.mail:jakarta.mail-api", InfraKind::Smtp),
    ("javax.mail:javax.mail-api", InfraKind::Smtp),
    ("io.quarkus:quarkus-mailer", InfraKind::Smtp),
    ("com.openai:openai-java", InfraKind::Openai),
    ("com.anthropic:anthropic-java", InfraKind::Anthropic),
];

/// Java package prefixes → infrastructure, for imports (`org.springframework.kafka.core.KafkaTemplate`).
const NAMESPACE_PACKAGES: &[(&str, InfraKind)] = &[
    ("org.springframework.kafka", InfraKind::Kafka),
    ("org.apache.kafka", InfraKind::Kafka),
    ("io.micronaut.configuration.kafka", InfraKind::Kafka),
    ("io.micronaut.kafka", InfraKind::Kafka),
    ("org.springframework.amqp", InfraKind::Rabbitmq),
    ("com.rabbitmq.client", InfraKind::Rabbitmq),
    ("io.micronaut.rabbitmq", InfraKind::Rabbitmq),
    // C# namespaces. The EF Core provider names the store, not `EntityFrameworkCore` itself.
    ("Npgsql", InfraKind::Postgres),
    ("MySqlConnector", InfraKind::Mysql),
    ("MySql.Data", InfraKind::Mysql),
    ("Microsoft.Data.Sqlite", InfraKind::Sqlite),
    ("StackExchange.Redis", InfraKind::Redis),
    ("MongoDB.Driver", InfraKind::Mongodb),
    ("Amazon.S3", InfraKind::S3),
    ("Amazon.DynamoDBv2", InfraKind::Dynamodb),
    ("Amazon.SQS", InfraKind::Sqs),
    ("Confluent.Kafka", InfraKind::Kafka),
    ("RabbitMQ.Client", InfraKind::Rabbitmq),
    ("NATS.Client", InfraKind::Nats),
    ("Nest", InfraKind::Elasticsearch),
    ("Elastic.Clients.Elasticsearch", InfraKind::Elasticsearch),
    ("Stripe", InfraKind::Stripe),
    ("SendGrid", InfraKind::Sendgrid),
    ("Twilio", InfraKind::Twilio),
    ("OpenAI", InfraKind::Openai),
    ("Anthropic", InfraKind::Anthropic),
    ("SlackNet", InfraKind::Slack),
    ("org.springframework.data.mongodb", InfraKind::Mongodb),
    ("com.mongodb", InfraKind::Mongodb),
    ("io.quarkus.mongodb", InfraKind::Mongodb),
    ("org.springframework.data.redis", InfraKind::Redis),
    ("redis.clients.jedis", InfraKind::Redis),
    ("io.lettuce.core", InfraKind::Redis),
    ("org.redisson", InfraKind::Redis),
    ("io.quarkus.redis", InfraKind::Redis),
    ("org.springframework.data.elasticsearch", InfraKind::Elasticsearch),
    ("co.elastic.clients.elasticsearch", InfraKind::Elasticsearch),
    ("org.elasticsearch.client", InfraKind::Elasticsearch),
    ("software.amazon.awssdk.services.s3", InfraKind::S3),
    ("com.amazonaws.services.s3", InfraKind::S3),
    ("io.minio", InfraKind::S3),
    ("software.amazon.awssdk.services.sqs", InfraKind::Sqs),
    ("io.awspring.cloud.sqs", InfraKind::Sqs),
    ("software.amazon.awssdk.services.dynamodb", InfraKind::Dynamodb),
    ("com.google.cloud.firestore", InfraKind::Firestore),
    ("com.google.cloud.pubsub", InfraKind::Pubsub),
    ("io.nats.client", InfraKind::Nats),
    ("com.stripe", InfraKind::Stripe),
    ("com.sendgrid", InfraKind::Sendgrid),
    ("com.twilio", InfraKind::Twilio),
    ("com.slack.api", InfraKind::Slack),
    ("org.springframework.mail", InfraKind::Smtp),
    ("javax.mail", InfraKind::Smtp),
    ("jakarta.mail", InfraKind::Smtp),
    ("io.quarkus.mailer", InfraKind::Smtp),
    ("com.openai", InfraKind::Openai),
    ("com.anthropic", InfraKind::Anthropic),
];

fn namespace_prefix<'a, T: Copy>(table: &'a [(&'a str, T)], spec: &str) -> Option<T> {
    table.iter().find_map(|(p, v)| (spec == *p || spec.starts_with(&format!("{p}."))).then_some(*v))
}

/// Infrastructure named by a dotted namespace import (JVM, C#).
pub fn infra_for_namespace(spec: &str) -> Option<InfraKind> {
    namespace_prefix(NAMESPACE_PACKAGES, spec)
}

/// Go module paths carry major-version suffixes (`/v76`); match by prefix.
pub fn infra_for_package(name: &str) -> Option<InfraKind> {
    // Python distributions use dashes where import names use underscores.
    let lower = name.to_lowercase().replace('_', "-");
    LIBRARIES.iter().find_map(|(pkg, kind)| {
        let pkg_l = pkg.to_lowercase();
        let hit = if pkg.contains('/') && pkg.contains('.') {
            lower == pkg_l || lower.starts_with(&format!("{pkg_l}/"))
        } else {
            lower == pkg_l
        };
        hit.then_some(*kind)
    })
}

/// Maps an import specifier to its package: `@scope/pkg/sub` → `@scope/pkg`,
/// `stripe.checkout` → `stripe`, Go paths are returned whole.
pub fn import_package(spec: &str) -> &str {
    if spec.contains('/') && spec.split('/').next().is_some_and(|h| h.contains('.')) {
        return spec;
    }
    if spec.starts_with('@') {
        let mut parts = spec.splitn(3, '/');
        let a = parts.next().unwrap_or("");
        let b = parts.next().unwrap_or("");
        return &spec[..(a.len() + 1 + b.len()).min(spec.len())];
    }
    spec.split(['/', '.', ':']).next().unwrap_or(spec)
}

/// An image that watches, administers or fronts a datastore without being one.
///
/// The names match by substring, so `postgres-exporter` was documented as a
/// PostgreSQL database and `kafka-ui` as an event bus — a Prometheus exporter
/// and a web console drawn as the store the system depends on.
fn is_companion_tool(name: &str) -> bool {
    const SUFFIXES: &[&str] =
        &["-exporter", "-ui", "-admin", "-console", "-manager", "-operator", "-backup", "-init", "-migrate"];
    const TOOLS: &[&str] = &[
        "pgadmin",
        "adminer",
        "phpmyadmin",
        "mongo-express",
        "redisinsight",
        "redis-commander",
        "kafdrop",
        "akhq",
        "kibana",
        "grafana",
        "prometheus",
    ];
    SUFFIXES.iter().any(|x| name.ends_with(x)) || TOOLS.iter().any(|t| name.contains(t))
}

pub fn infra_for_image(image: &str) -> Option<InfraKind> {
    let name = image.rsplit('/').next().unwrap_or(image).split(':').next().unwrap_or("").to_lowercase();
    let full = image.to_lowercase();
    if is_companion_tool(&name) {
        return None;
    }
    Some(match name.as_str() {
        n if n.starts_with("postgres") || n == "postgis" || n.contains("timescale") => InfraKind::Postgres,
        "mysql" | "mariadb" => InfraKind::Mysql,
        "mongo" | "mongodb" => InfraKind::Mongodb,
        "redis" | "valkey" | "keydb" | "dragonfly" => InfraKind::Redis,
        "elasticsearch" | "opensearch" => InfraKind::Elasticsearch,
        "minio" | "localstack" => InfraKind::S3,
        "nats" => InfraKind::Nats,
        n if n.contains("firestore") => InfraKind::Firestore,
        n if n.contains("dynamodb") => InfraKind::Dynamodb,
        "rabbitmq" => InfraKind::Rabbitmq,
        n if n.contains("kafka") || n.contains("redpanda") || full.contains("redpanda") => InfraKind::Kafka,
        "mailhog" | "mailpit" | "maildev" | "mailcatcher" | "inbucket" => InfraKind::Smtp,
        _ => return None,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum FrameworkRole {
    HttpServer,
    WebClient,
    Cli,
}

const FRAMEWORKS: &[(&str, &str, FrameworkRole)] = &[
    ("express", "Express", FrameworkRole::HttpServer),
    ("fastify", "Fastify", FrameworkRole::HttpServer),
    ("koa", "Koa", FrameworkRole::HttpServer),
    ("hono", "Hono", FrameworkRole::HttpServer),
    ("@nestjs/core", "NestJS", FrameworkRole::HttpServer),
    ("axum", "Axum", FrameworkRole::HttpServer),
    ("actix-web", "Actix Web", FrameworkRole::HttpServer),
    ("rocket", "Rocket", FrameworkRole::HttpServer),
    ("warp", "warp", FrameworkRole::HttpServer),
    ("poem", "Poem", FrameworkRole::HttpServer),
    ("github.com/gin-gonic/gin", "Gin", FrameworkRole::HttpServer),
    ("github.com/labstack/echo", "Echo", FrameworkRole::HttpServer),
    ("github.com/gofiber/fiber", "Fiber", FrameworkRole::HttpServer),
    ("github.com/go-chi/chi", "chi", FrameworkRole::HttpServer),
    ("github.com/gorilla/mux", "gorilla/mux", FrameworkRole::HttpServer),
    ("net/http", "net/http", FrameworkRole::HttpServer),
    ("fastapi", "FastAPI", FrameworkRole::HttpServer),
    ("flask", "Flask", FrameworkRole::HttpServer),
    ("django", "Django", FrameworkRole::HttpServer),
    ("starlette", "Starlette", FrameworkRole::HttpServer),
    ("aiohttp", "aiohttp", FrameworkRole::HttpServer),
    ("next", "Next.js", FrameworkRole::WebClient),
    ("react", "React", FrameworkRole::WebClient),
    ("vue", "Vue", FrameworkRole::WebClient),
    ("svelte", "Svelte", FrameworkRole::WebClient),
    ("@angular/core", "Angular", FrameworkRole::WebClient),
    ("solid-js", "Solid", FrameworkRole::WebClient),
    ("vite", "Vite", FrameworkRole::WebClient),
    ("clap", "clap", FrameworkRole::Cli),
    ("cobra", "Cobra", FrameworkRole::Cli),
    ("github.com/spf13/cobra", "Cobra", FrameworkRole::Cli),
    ("click", "Click", FrameworkRole::Cli),
    ("typer", "Typer", FrameworkRole::Cli),
    ("commander", "Commander", FrameworkRole::Cli),
    // JVM
    ("org.springframework.boot:spring-boot-starter-web", "Spring MVC", FrameworkRole::HttpServer),
    // Spring Boot 4 renamed the servlet starter.
    ("org.springframework.boot:spring-boot-starter-webmvc", "Spring MVC", FrameworkRole::HttpServer),
    ("org.springframework.boot:spring-boot-starter-webflux", "Spring WebFlux", FrameworkRole::HttpServer),
    ("org.springframework.boot:spring-boot-starter-jersey", "Jersey", FrameworkRole::HttpServer),
    ("org.springframework.cloud:spring-cloud-starter-gateway", "Spring Cloud Gateway", FrameworkRole::HttpServer),
    ("org.springframework.cloud:spring-cloud-starter-gateway-mvc", "Spring Cloud Gateway", FrameworkRole::HttpServer),
    ("org.springframework.cloud:spring-cloud-starter-netflix-zuul", "Zuul", FrameworkRole::HttpServer),
    (
        "org.springframework.cloud:spring-cloud-starter-netflix-eureka-server",
        "Eureka Server",
        FrameworkRole::HttpServer,
    ),
    ("org.springframework.cloud:spring-cloud-config-server", "Spring Cloud Config Server", FrameworkRole::HttpServer),
    (
        "org.springframework.cloud:spring-cloud-starter-netflix-hystrix-dashboard",
        "Hystrix Dashboard",
        FrameworkRole::HttpServer,
    ),
    ("org.springframework.cloud:spring-cloud-starter-netflix-turbine-stream", "Turbine", FrameworkRole::HttpServer),
    ("de.codecentric:spring-boot-admin-starter-server", "Spring Boot Admin", FrameworkRole::HttpServer),
    ("io.quarkus:quarkus-resteasy", "Quarkus", FrameworkRole::HttpServer),
    ("io.quarkus:quarkus-resteasy-reactive", "Quarkus", FrameworkRole::HttpServer),
    ("io.quarkus:quarkus-resteasy-jackson", "Quarkus", FrameworkRole::HttpServer),
    ("io.quarkus:quarkus-resteasy-reactive-jackson", "Quarkus", FrameworkRole::HttpServer),
    ("io.quarkus:quarkus-rest", "Quarkus", FrameworkRole::HttpServer),
    ("io.quarkus:quarkus-rest-jackson", "Quarkus", FrameworkRole::HttpServer),
    ("io.quarkus:quarkus-vertx-http", "Quarkus", FrameworkRole::HttpServer),
    ("io.micronaut:micronaut-http-server-netty", "Micronaut", FrameworkRole::HttpServer),
    ("io.micronaut:micronaut-http-server", "Micronaut", FrameworkRole::HttpServer),
    ("io.dropwizard:dropwizard-core", "Dropwizard", FrameworkRole::HttpServer),
    ("io.helidon.webserver:helidon-webserver", "Helidon", FrameworkRole::HttpServer),
    ("io.helidon.microprofile.bundles:helidon-microprofile", "Helidon MP", FrameworkRole::HttpServer),
    ("io.helidon.microprofile.bundles:helidon-microprofile-core", "Helidon MP", FrameworkRole::HttpServer),
    ("io.javalin:javalin", "Javalin", FrameworkRole::HttpServer),
    ("io.vertx:vertx-web", "Vert.x", FrameworkRole::HttpServer),
    ("com.sparkjava:spark-core", "Spark Java", FrameworkRole::HttpServer),
    ("org.glassfish.jersey.core:jersey-server", "Jersey", FrameworkRole::HttpServer),
    ("org.glassfish.jersey.containers:jersey-container-servlet", "Jersey", FrameworkRole::HttpServer),
    ("org.eclipse.microprofile:microprofile", "MicroProfile", FrameworkRole::HttpServer),
    ("io.ktor:ktor-server-core", "Ktor", FrameworkRole::HttpServer),
    ("io.ktor:ktor-server-netty", "Ktor", FrameworkRole::HttpServer),
    ("io.ktor:ktor-server-cio", "Ktor", FrameworkRole::HttpServer),
    ("io.ktor:ktor-server-tomcat", "Ktor", FrameworkRole::HttpServer),
    ("io.ktor:ktor-server-jetty", "Ktor", FrameworkRole::HttpServer),
    // `<Project Sdk="Microsoft.NET.Sdk.Web">` is what declares a .NET web app;
    // ASP.NET Core itself ships in the shared framework, so there is no
    // `PackageReference` to find.
    ("microsoft.net.sdk.web", "ASP.NET Core", FrameworkRole::HttpServer),
    ("microsoft.aspnetcore.app", "ASP.NET Core", FrameworkRole::HttpServer),
    ("microsoft.net.sdk.worker", "Worker Service", FrameworkRole::Cli),
    ("info.picocli:picocli", "picocli", FrameworkRole::Cli),
    ("org.springframework.shell:spring-shell-starter", "Spring Shell", FrameworkRole::Cli),
];

/// Dotted namespace prefixes → framework, for languages where an import names
/// a namespace rather than a file: JVM modules whose build file inherits its
/// dependencies (a parent POM), and C#, where the `.csproj` lists NuGet ids
/// that need not resemble the namespaces used.
const NAMESPACE_FRAMEWORKS: &[(&str, (&str, FrameworkRole))] = &[
    ("org.springframework.web.bind.annotation", ("Spring MVC", FrameworkRole::HttpServer)),
    ("org.springframework.web.reactive", ("Spring WebFlux", FrameworkRole::HttpServer)),
    ("org.springframework.cloud.gateway", ("Spring Cloud Gateway", FrameworkRole::HttpServer)),
    ("org.springframework.cloud.netflix.zuul", ("Zuul", FrameworkRole::HttpServer)),
    ("org.springframework.cloud.netflix.eureka.server", ("Eureka Server", FrameworkRole::HttpServer)),
    ("org.springframework.cloud.config.server", ("Spring Cloud Config Server", FrameworkRole::HttpServer)),
    ("jakarta.ws.rs", ("JAX-RS", FrameworkRole::HttpServer)),
    ("javax.ws.rs", ("JAX-RS", FrameworkRole::HttpServer)),
    ("io.micronaut.http.annotation", ("Micronaut", FrameworkRole::HttpServer)),
    ("io.dropwizard", ("Dropwizard", FrameworkRole::HttpServer)),
    ("io.javalin", ("Javalin", FrameworkRole::HttpServer)),
    ("io.vertx.ext.web", ("Vert.x", FrameworkRole::HttpServer)),
    ("io.helidon.webserver", ("Helidon", FrameworkRole::HttpServer)),
    ("io.ktor.server.routing", ("Ktor", FrameworkRole::HttpServer)),
    ("io.ktor.server.engine", ("Ktor", FrameworkRole::HttpServer)),
    ("io.ktor.server.application", ("Ktor", FrameworkRole::HttpServer)),
    ("picocli", ("picocli", FrameworkRole::Cli)),
    ("Microsoft.AspNetCore.Mvc", ("ASP.NET Core", FrameworkRole::HttpServer)),
    ("Microsoft.AspNetCore.Builder", ("ASP.NET Core", FrameworkRole::HttpServer)),
    ("Microsoft.AspNetCore.Http", ("ASP.NET Core", FrameworkRole::HttpServer)),
    ("Microsoft.AspNetCore.Routing", ("ASP.NET Core", FrameworkRole::HttpServer)),
];

/// Services that support the platform rather than serve requests: role
/// shown on the card and the verb for edges into them.
pub fn platform_service(framework: &str) -> Option<(&'static str, &'static str)> {
    Some(match framework {
        "Spring Cloud Config Server" => ("Configuration server", "fetches config"),
        "Eureka Server" => ("Service registry", "registers with"),
        "Hystrix Dashboard" => ("Monitoring dashboard", "monitored by"),
        "Turbine" => ("Metrics aggregator", "streams metrics"),
        "Spring Boot Admin" => ("Admin console", "reports to"),
        _ => return None,
    })
}

/// Role wording for gateways, which otherwise read as plain HTTP services.
pub fn gateway_framework(framework: &str) -> bool {
    matches!(framework, "Zuul" | "Spring Cloud Gateway")
}

pub fn framework_for_namespace(spec: &str) -> Option<(&'static str, FrameworkRole)> {
    namespace_prefix(NAMESPACE_FRAMEWORKS, spec)
}

/// Database access layers that don't name the database themselves.
pub fn is_orm(name: &str) -> bool {
    let n = name.to_lowercase();
    [
        "sqlmodel",
        "sqlalchemy",
        "django",
        "peewee",
        "tortoise",
        "typeorm",
        "sequelize",
        "drizzle-orm",
        "knex",
        "kysely",
        "gorm.io/gorm",
        "github.com/jmoiron/sqlx",
        "database/sql",
        "sea-orm",
        // JVM persistence (packages and coordinates)
        "jakarta.persistence",
        "javax.persistence",
        "org.hibernate",
        "org.springframework.data.jpa",
        "org.springframework.data.jdbc",
        "org.springframework.data.r2dbc",
        "org.springframework.jdbc",
        "io.micronaut.data",
        "io.quarkus.hibernate",
        "org.jooq",
        "org.mybatis",
        "org.jdbi",
        // .NET data access
        "microsoft.entityframeworkcore",
        "npgsql.entityframeworkcore.postgresql",
        "pomelo.entityframeworkcore.mysql",
        "dapper",
        "nhibernate",
        "linq2db",
    ]
    .iter()
    .any(|p| n == *p || n.starts_with(&format!("{p}/")) || n.starts_with(&format!("{p}.")))
}

/// Model Context Protocol server SDKs.
pub fn is_mcp_sdk(name: &str) -> bool {
    let n = name.to_lowercase();
    [
        "@modelcontextprotocol/sdk",
        "mcp",
        "fastmcp",
        "rmcp",
        "github.com/mark3labs/mcp-go",
        "github.com/modelcontextprotocol/go-sdk",
    ]
    .iter()
    .any(|p| n == *p || (p.contains('.') && n.starts_with(&format!("{p}/"))))
}

pub fn framework_for_package(name: &str) -> Option<(&'static str, FrameworkRole)> {
    let lower = name.to_lowercase();
    FRAMEWORKS.iter().find_map(|(pkg, label, role)| {
        let hit = lower == *pkg || (pkg.contains('.') && lower.starts_with(&format!("{pkg}/")));
        hit.then_some((*label, *role))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn go_modules_match_with_major_suffix() {
        assert_eq!(infra_for_package("github.com/stripe/stripe-go/v76"), Some(InfraKind::Stripe));
        assert_eq!(infra_for_package("github.com/jackc/pgx/v5/pgxpool"), Some(InfraKind::Postgres));
        assert_eq!(infra_for_package("github.com/jackc/pgxfoo"), None);
    }

    #[test]
    fn import_specifiers_reduce_to_packages() {
        assert_eq!(import_package("@sendgrid/mail/src/x"), "@sendgrid/mail");
        assert_eq!(import_package("sendgrid.helpers.mail"), "sendgrid");
        assert_eq!(import_package("sqlx::postgres::PgPool"), "sqlx");
        assert_eq!(import_package("github.com/segmentio/kafka-go"), "github.com/segmentio/kafka-go");
    }

    #[test]
    fn images_classify() {
        assert_eq!(infra_for_image("postgres:16"), Some(InfraKind::Postgres));
        assert_eq!(infra_for_image("docker.redpanda.com/redpandadata/redpanda:v24"), Some(InfraKind::Kafka));
        assert_eq!(infra_for_image("bitnami/redis"), Some(InfraKind::Redis));
        // A tool that watches or administers a store is not the store: these
        // used to be documented as the database the system depends on.
        assert_eq!(infra_for_image("prometheuscommunity/postgres-exporter"), None);
        assert_eq!(infra_for_image("provectuslabs/kafka-ui:latest"), None);
        assert_eq!(infra_for_image("dpage/pgadmin4"), None);
        assert_eq!(infra_for_image("mongo-express:1.0"), None);
        assert_eq!(infra_for_image("nginx:1"), None);
    }
}

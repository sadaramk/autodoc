plugins {
    id("org.springframework.boot") version "3.3.0"
    kotlin("jvm") version "2.0.0"
}

dependencies {
    implementation(project(":pricing"))
    implementation("org.springframework.boot:spring-boot-starter-web")
    implementation("org.springframework.cloud:spring-cloud-starter-openfeign")
    implementation("io.ktor:ktor-client-core:2.3.11")
}

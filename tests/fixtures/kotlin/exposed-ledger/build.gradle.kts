plugins {
    kotlin("jvm") version "2.0.0"
    application
}

dependencies {
    implementation("org.jetbrains.exposed:exposed-core:0.52.0")
    implementation("org.jetbrains.exposed:exposed-dao:0.52.0")
    implementation("org.jetbrains.exposed:exposed-jdbc:0.52.0")
    implementation("org.jetbrains.exposed:exposed-java-time:0.52.0")
    implementation("org.postgresql:postgresql:42.7.3")
}

application {
    mainClass.set("com.acme.ledger.MainKt")
}

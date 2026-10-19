package dev.rivqen.webviewlab

interface Platform {
    val name: String
}

expect fun getPlatform(): Platform
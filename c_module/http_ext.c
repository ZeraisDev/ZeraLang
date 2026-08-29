#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <curl/curl.h>

// This struct stores the downloaded data in memory
struct MemoryStruct {
    char *memory;
    size_t size;
};

// This function is called by curl as data comes in
static size_t WriteMemoryCallback(void *contents, size_t size, size_t nmemb, void *userp) {
    size_t realsize = size * nmemb;
    struct MemoryStruct *mem = (struct MemoryStruct *)userp;

    char *ptr = realloc(mem->memory, mem->size + realsize + 1);
    if(!ptr) return 0;

    mem->memory = ptr;
    memcpy(&(mem->memory[mem->size]), contents, realsize);
    mem->size += realsize;
    mem->memory[mem->size] = 0;

    return realsize;
}

// The simple function Zeralang will call
const char* zera_http_get(const char* url) {
    CURL *curl_handle;
    CURLcode res;

    struct MemoryStruct chunk;
    chunk.memory = malloc(1);
    chunk.size = 0;

    curl_global_init(CURL_GLOBAL_ALL);
    curl_handle = curl_easy_init();

    curl_easy_setopt(curl_handle, CURLOPT_URL, url);
    curl_easy_setopt(curl_handle, CURLOPT_WRITEFUNCTION, WriteMemoryCallback);
    curl_easy_setopt(curl_handle, CURLOPT_WRITEDATA, (void *)&chunk);
    curl_easy_setopt(curl_handle, CURLOPT_FOLLOWLOCATION, 1L); // Follow redirects
    // Many websites require a User-Agent to not block you
    curl_easy_setopt(curl_handle, CURLOPT_USERAGENT, "Zeralang/1.0");

    res = curl_easy_perform(curl_handle);

    if(res != CURLE_OK) {
        char* error_msg = malloc(100);
        sprintf(error_msg, "CURL Error: %s", curl_easy_strerror(res));
        curl_easy_cleanup(curl_handle);
        curl_global_cleanup();
        return error_msg;
    }

    curl_easy_cleanup(curl_handle);
    curl_global_cleanup();

    return chunk.memory;
}

#pragma once

#include "frida-gum.h"

void gum_bindings_init();

GumInterceptor *gum_bindings_interceptor_obtain();

GumReplaceReturn gum_bindings_interceptor_replace_fast(
    GumInterceptor *self,
    gpointer function_address,
    gpointer replacement_function,
    gpointer *original_function);

void gum_bindings_interceptor_begin_transaction(GumInterceptor *self);
void gum_bindings_interceptor_end_transaction(GumInterceptor *self);

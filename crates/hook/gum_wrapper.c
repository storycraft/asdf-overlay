#include "gum_wrapper.h"

void gum_bindings_init()
{
    gum_init_embedded();
}

GumInterceptor *gum_bindings_interceptor_obtain()
{
    return gum_interceptor_obtain();
}

GumReplaceReturn gum_bindings_interceptor_replace_fast(
    GumInterceptor *self,
    gpointer function_address,
    gpointer replacement_function,
    gpointer *original_function)
{
    const GumInterceptorOptions OPTIONS = {
        .scenario = GUM_INTERCEPTOR_SCENARIO_ONLINE,
        .relocation_policy = GUM_RELOCATION_FORCED,
    };

    return gum_interceptor_replace_fast(
        self,
        function_address,
        replacement_function,
        original_function,
        &OPTIONS);
}

void gum_bindings_interceptor_begin_transaction(GumInterceptor *self)
{
    gum_interceptor_begin_transaction(self);
}

void gum_bindings_interceptor_end_transaction(GumInterceptor *self)
{
    gum_interceptor_end_transaction(self);
}

/*
 * glib_native.h — C ABI header for glib-native (Phase 13)
 *
 * This library is free software; you can redistribute it and/or modify it
 * under the terms of the GNU Lesser General Public License as published by
 * the Free Software Foundation; either version 2.1 of the License, or
 * (at your option) any later version.
 *
 * This library is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the GNU
 * Lesser General Public License for more details.
 *
 * You should have received a copy of the GNU Lesser General Public License
 * along with this library; if not, see <https://www.gnu.org/licenses/>.
 */

#ifndef GLIB_NATIVE_H
#define GLIB_NATIVE_H

#ifdef __cplusplus
extern "C" {
#endif

#include <stddef.h>
#include <stdint.h>

/* ── Fundamental GLib-compatible types ───────────────────────────────── */

typedef size_t  GType;
typedef void   *gpointer;
typedef const void *gconstpointer;
typedef int     gboolean;
typedef char    gchar;
typedef unsigned char guchar;
typedef int32_t gint;
typedef uint32_t guint;
typedef int64_t gint64;
typedef uint64_t guint64;
typedef float   gfloat;
typedef double  gdouble;
typedef size_t  gsize;
typedef uint32_t GQuark;

typedef struct _GValue GValue;
typedef struct _GError GError;

struct _GValue {
    unsigned char _storage[64];
};

struct _GError {
    GQuark domain;
    gint   code;
    gchar *message;
};

typedef void (*GWeakNotify)(gpointer data, gpointer where_the_object_was_lived);
typedef void (*GSignalCMarshaller)(gpointer instance, gpointer user_data);

/* ── Type system ─────────────────────────────────────────────────────── */

void     g_type_init(void);
GType    g_type_from_name(const char *name);
const char *g_type_name(GType type);
gboolean g_type_is_a(GType type, GType is_a_type);
GType    g_type_fundamental(GType type_id);
guint    g_type_get_type_registration_serial(void);

/* ── Memory (`gmem.h`) ───────────────────────────────────────────────── */

gpointer g_malloc(gsize n_bytes);
gpointer g_malloc0(gsize n_bytes);
gpointer g_try_malloc(gsize n_bytes);
gpointer g_try_malloc0(gsize n_bytes);
gpointer g_realloc(gpointer mem, gsize n_bytes);
gpointer g_try_realloc(gpointer mem, gsize n_bytes);
gpointer g_malloc_n(gsize n_blocks, gsize n_block_bytes);
gpointer g_malloc0_n(gsize n_blocks, gsize n_block_bytes);
gpointer g_try_malloc_n(gsize n_blocks, gsize n_block_bytes);
gpointer g_try_malloc0_n(gsize n_blocks, gsize n_block_bytes);
gpointer g_realloc_n(gpointer mem, gsize n_blocks, gsize n_block_bytes);
gpointer g_try_realloc_n(gpointer mem, gsize n_blocks, gsize n_block_bytes);
void     g_free(gpointer mem);
gpointer g_memdup2(gconstpointer mem, gsize byte_size);
gpointer g_memdup(gconstpointer mem, guint byte_size);

/* ── Strings ─────────────────────────────────────────────────────────── */

gchar   *g_strdup(const gchar *str);
gchar   *g_strndup(const gchar *str, gsize n);

/* ── Quarks ──────────────────────────────────────────────────────────── */

GQuark   g_quark_from_string(const gchar *string);
GQuark   g_quark_try_string(const gchar *string);
const gchar *g_quark_to_string(GQuark quark);

/* ── GValue ──────────────────────────────────────────────────────────── */

void     g_value_init(GValue *value, GType type);
void     g_value_unset(GValue *value);
void     g_value_reset(GValue *value);
void     g_value_copy(const GValue *src_value, GValue *dest_value);
GType    g_value_get_type(const GValue *value);

gboolean g_value_get_boolean(const GValue *value);
void     g_value_set_boolean(GValue *value, gboolean v);
gint     g_value_get_int(const GValue *value);
void     g_value_set_int(GValue *value, gint v);
guint    g_value_get_uint(const GValue *value);
void     g_value_set_uint(GValue *value, guint v);
gint64   g_value_get_int64(const GValue *value);
void     g_value_set_int64(GValue *value, gint64 v);
guint64  g_value_get_uint64(const GValue *value);
void     g_value_set_uint64(GValue *value, guint64 v);
gfloat   g_value_get_float(const GValue *value);
void     g_value_set_float(GValue *value, gfloat v);
gdouble  g_value_get_double(const GValue *value);
void     g_value_set_double(GValue *value, gdouble v);
gchar    g_value_get_char(const GValue *value);
void     g_value_set_char(GValue *value, gchar v);
guchar   g_value_get_uchar(const GValue *value);
void     g_value_set_uchar(GValue *value, guchar v);
gint64   g_value_get_long(const GValue *value);
void     g_value_set_long(GValue *value, gint64 v);
guint64  g_value_get_ulong(const GValue *value);
void     g_value_set_ulong(GValue *value, guint64 v);
gint     g_value_get_enum(const GValue *value);
void     g_value_set_enum(GValue *value, gint v);
guint    g_value_get_flags(const GValue *value);
void     g_value_set_flags(GValue *value, guint v);
const gchar *g_value_get_string(const GValue *value);
void     g_value_set_string(GValue *value, const gchar *v);
void     g_value_set_static_string(GValue *value, const gchar *v);
void     g_value_take_string(GValue *value, gchar *v);
void     g_value_set_string_take_ownership(GValue *value, gchar *v);
gchar   *g_value_dup_string(const GValue *value);
gpointer g_value_get_pointer(const GValue *value);
void     g_value_set_pointer(GValue *value, gpointer v);

/* ── GObject ─────────────────────────────────────────────────────────── */

gpointer g_object_ref(gpointer object);
void     g_object_unref(gpointer object);
gpointer g_object_ref_sink(gpointer object);
gpointer g_object_get_qdata(gpointer object, GQuark quark);
void     g_object_set_qdata(gpointer object, GQuark quark, gpointer data);
void     g_object_weak_ref(gpointer object, GWeakNotify notify, gpointer data);

/* ── Signals & params ────────────────────────────────────────────────── */

guint64  g_signal_connect_data(gpointer instance,
                               const char *detailed_signal,
                               GSignalCMarshaller c_handler,
                               gpointer data,
                               GWeakNotify destroy_data,
                               guint connect_flags);
gpointer g_param_spec_int(const char *name,
                          const char *nick,
                          const char *blurb,
                          gint minimum,
                          gint maximum,
                          gint default_value,
                          guint flags);

/* ── Errors ──────────────────────────────────────────────────────────── */

void g_set_error(GError **err,
                 GQuark domain,
                 gint code,
                 const char *format,
                 ...);
void g_clear_error(GError **err);
void g_error_free(GError *error);
void g_propagate_error(GError **dest, GError *src);

#ifdef __cplusplus
}
#endif

#endif /* GLIB_NATIVE_H */

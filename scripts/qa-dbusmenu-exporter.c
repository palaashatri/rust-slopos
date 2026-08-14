/*
 * QA-only DBusMenu exporter fixture.
 *
 * This helper is compiled into /tmp by the Ubuntu Xvfb gate and is never
 * installed or shipped.  It owns no application UI and implements only the
 * small, standard com.canonical.dbusmenu surface needed to prove SLOPOS's
 * capability-aware importer.  The test supplies the helper's unique session
 * bus name as _GTK_UNIQUE_BUS_NAME on a real Mousepad window.
 */

#include <dbus/dbus.h>
#include <errno.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

#define DBUSMENU_INTERFACE "com.canonical.dbusmenu"
#define DBUSMENU_PATH "/org/slopos/qa/dbusmenu"

struct Fixture {
    const char *bus_file;
    const char *event_file;
};

static void fail_dbus(const char *operation, DBusError *error) {
    fprintf(stderr, "qa-dbusmenu-exporter: %s: %s\n", operation,
            error && dbus_error_is_set(error) ? error->message : "unknown error");
    if (error && dbus_error_is_set(error)) {
        dbus_error_free(error);
    }
    exit(1);
}

static void append_property(DBusMessageIter *properties, const char *key,
                            const char *value) {
    DBusMessageIter entry;
    DBusMessageIter variant;
    dbus_message_iter_open_container(properties, DBUS_TYPE_DICT_ENTRY, NULL,
                                      &entry);
    dbus_message_iter_append_basic(&entry, DBUS_TYPE_STRING, &key);
    dbus_message_iter_open_container(&entry, DBUS_TYPE_VARIANT, "s", &variant);
    dbus_message_iter_append_basic(&variant, DBUS_TYPE_STRING, &value);
    dbus_message_iter_close_container(&entry, &variant);
    dbus_message_iter_close_container(properties, &entry);
}

static void append_item(DBusMessageIter *children, dbus_int32_t id,
                        const char *label) {
    DBusMessageIter variant;
    DBusMessageIter item;
    DBusMessageIter properties;
    DBusMessageIter item_children;

    /* DBusMenu's children are av: each child node is a variant containing a
     * (ia{sv}av) structure. */
    dbus_message_iter_open_container(children, DBUS_TYPE_VARIANT,
                                      "(ia{sv}av)", &variant);
    dbus_message_iter_open_container(&variant, DBUS_TYPE_STRUCT, NULL, &item);
    dbus_message_iter_append_basic(&item, DBUS_TYPE_INT32, &id);
    dbus_message_iter_open_container(&item, DBUS_TYPE_ARRAY, "{sv}",
                                      &properties);
    append_property(&properties, "label", label);
    dbus_message_iter_close_container(&item, &properties);
    dbus_message_iter_open_container(&item, DBUS_TYPE_ARRAY, "v",
                                      &item_children);
    dbus_message_iter_close_container(&item, &item_children);
    dbus_message_iter_close_container(&variant, &item);
    dbus_message_iter_close_container(children, &variant);
}

static DBusMessage *layout_reply(DBusMessage *call) {
    DBusMessage *reply = dbus_message_new_method_return(call);
    DBusMessageIter body;
    DBusMessageIter root;
    DBusMessageIter properties;
    DBusMessageIter children;
    dbus_uint32_t revision = 1;
    dbus_int32_t root_id = 0;

    if (!reply) {
        return NULL;
    }
    dbus_message_iter_init_append(reply, &body);
    dbus_message_iter_append_basic(&body, DBUS_TYPE_UINT32, &revision);
    dbus_message_iter_open_container(&body, DBUS_TYPE_STRUCT, NULL, &root);
    dbus_message_iter_append_basic(&root, DBUS_TYPE_INT32, &root_id);
    dbus_message_iter_open_container(&root, DBUS_TYPE_ARRAY, "{sv}",
                                      &properties);
    dbus_message_iter_close_container(&root, &properties);
    dbus_message_iter_open_container(&root, DBUS_TYPE_ARRAY, "v", &children);
    append_item(&children, 1, "QA Action");
    dbus_message_iter_close_container(&root, &children);
    dbus_message_iter_close_container(&body, &root);
    return reply;
}

static DBusHandlerResult handle_message(DBusConnection *connection,
                                         DBusMessage *message, void *user_data) {
    struct Fixture *fixture = user_data;
    DBusMessage *reply;
    DBusError error;
    dbus_int32_t item_id;
    const char *event = NULL;

    if (dbus_message_is_method_call(message, DBUSMENU_INTERFACE, "GetLayout") &&
        strcmp(dbus_message_get_path(message), DBUSMENU_PATH) == 0) {
        reply = layout_reply(message);
        if (!reply || !dbus_connection_send(connection, reply, NULL)) {
            if (reply) {
                dbus_message_unref(reply);
            }
            return DBUS_HANDLER_RESULT_NEED_MEMORY;
        }
        dbus_connection_flush(connection);
        dbus_message_unref(reply);
        return DBUS_HANDLER_RESULT_HANDLED;
    }

    if (dbus_message_is_method_call(message, DBUSMENU_INTERFACE, "Event") &&
        strcmp(dbus_message_get_path(message), DBUSMENU_PATH) == 0) {
        dbus_error_init(&error);
        if (!dbus_message_get_args(message, &error, DBUS_TYPE_INT32, &item_id,
                                   DBUS_TYPE_STRING, &event, DBUS_TYPE_INVALID)) {
            dbus_error_free(&error);
            reply = dbus_message_new_error(message, DBUS_ERROR_INVALID_ARGS,
                                            "Expected (isvu) DBusMenu Event");
        } else if (!event || strcmp(event, "clicked") != 0) {
            reply = dbus_message_new_error(message, DBUS_ERROR_INVALID_ARGS,
                                            "Only the DBusMenu clicked event is accepted");
        } else {
            FILE *events = fopen(fixture->event_file, "a");
            if (events) {
                fprintf(events, "clicked id=%d event=%s\n", item_id, event);
                fclose(events);
            }
            reply = dbus_message_new_method_return(message);
        }
        if (!reply || !dbus_connection_send(connection, reply, NULL)) {
            if (reply) {
                dbus_message_unref(reply);
            }
            return DBUS_HANDLER_RESULT_NEED_MEMORY;
        }
        dbus_connection_flush(connection);
        dbus_message_unref(reply);
        return DBUS_HANDLER_RESULT_HANDLED;
    }

    return DBUS_HANDLER_RESULT_NOT_YET_HANDLED;
}

int main(int argc, char **argv) {
    DBusConnection *connection;
    DBusError error;
    FILE *bus;
    struct Fixture fixture;
    const char *unique_name;

    if (argc != 3) {
        fprintf(stderr, "usage: %s BUS_NAME_FILE EVENT_FILE\n", argv[0]);
        return 2;
    }
    fixture.bus_file = argv[1];
    fixture.event_file = argv[2];
    dbus_error_init(&error);
    connection = dbus_bus_get(DBUS_BUS_SESSION, &error);
    if (!connection) {
        fail_dbus("connect session bus", &error);
    }
    dbus_connection_set_exit_on_disconnect(connection, FALSE);
    if (!dbus_connection_add_filter(connection, handle_message, &fixture, NULL)) {
        fprintf(stderr, "qa-dbusmenu-exporter: cannot install message filter\n");
        return 1;
    }
    unique_name = dbus_bus_get_unique_name(connection);
    if (!unique_name || unique_name[0] != ':') {
        fprintf(stderr, "qa-dbusmenu-exporter: no unique session bus name\n");
        return 1;
    }
    bus = fopen(fixture.bus_file, "w");
    if (!bus) {
        fprintf(stderr, "qa-dbusmenu-exporter: %s: %s\n", fixture.bus_file,
                strerror(errno));
        return 1;
    }
    fprintf(bus, "%s\n", unique_name);
    fclose(bus);
    /* Keep the service alive until the shell test's cleanup kills us. */
    while (dbus_connection_read_write_dispatch(connection, -1)) {
    }
    dbus_connection_remove_filter(connection, handle_message, &fixture);
    dbus_connection_unref(connection);
    return 0;
}

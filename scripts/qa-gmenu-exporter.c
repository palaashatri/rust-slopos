/*
 * QA-only org.gtk.Menus/org.gtk.Actions exporter fixture.
 *
 * This process is compiled into /tmp by the AppMenu QA harness and is never
 * installed.  Its object path deliberately looks like an upstream GMenu
 * exporter: Start([0]) returns linked groups with mnemonic labels and
 * app./win.-style action names, while DescribeAll/Activate expose the
 * corresponding action group.  The native GMenu importer should import the
 * headings and send a typed Actions activation.  The application
 * and window action groups intentionally live at distinct object paths so
 * prefix routing is exercised rather than inferred from the menu path.
 */

#include <dbus/dbus.h>
#include <errno.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define MENUS_INTERFACE "org.gtk.Menus"
#define ACTIONS_INTERFACE "org.gtk.Actions"
#define MENU_PATH "/org/slopos/qa/gmenu"
#define APPLICATION_PATH "/org/slopos/qa/gmenu/application"
#define WINDOW_PATH "/org/slopos/qa/gmenu/window"

struct Fixture {
    const char *bus_file;
    const char *event_file;
    dbus_bool_t mark_state;
    char radio_state[32];
};

static void fail_dbus(const char *operation, DBusError *error) {
    fprintf(stderr, "qa-gmenu-exporter: %s: %s\n", operation,
            error && dbus_error_is_set(error) ? error->message : "unknown error");
    if (error && dbus_error_is_set(error)) {
        dbus_error_free(error);
    }
    exit(1);
}

static void append_variant_string(DBusMessageIter *dict, const char *key,
                                  const char *value) {
    DBusMessageIter entry;
    DBusMessageIter variant;
    dbus_message_iter_open_container(dict, DBUS_TYPE_DICT_ENTRY, NULL, &entry);
    dbus_message_iter_append_basic(&entry, DBUS_TYPE_STRING, &key);
    dbus_message_iter_open_container(&entry, DBUS_TYPE_VARIANT, "s", &variant);
    dbus_message_iter_append_basic(&variant, DBUS_TYPE_STRING, &value);
    dbus_message_iter_close_container(&entry, &variant);
    dbus_message_iter_close_container(dict, &entry);
}

static void append_variant_link(DBusMessageIter *dict, const char *key,
                                dbus_uint32_t group, dbus_uint32_t menu) {
    DBusMessageIter entry;
    DBusMessageIter variant;
    DBusMessageIter pair;
    dbus_message_iter_open_container(dict, DBUS_TYPE_DICT_ENTRY, NULL, &entry);
    dbus_message_iter_append_basic(&entry, DBUS_TYPE_STRING, &key);
    dbus_message_iter_open_container(&entry, DBUS_TYPE_VARIANT, "(uu)",
                                     &variant);
    dbus_message_iter_open_container(&variant, DBUS_TYPE_STRUCT, NULL, &pair);
    dbus_message_iter_append_basic(&pair, DBUS_TYPE_UINT32, &group);
    dbus_message_iter_append_basic(&pair, DBUS_TYPE_UINT32, &menu);
    dbus_message_iter_close_container(&variant, &pair);
    dbus_message_iter_close_container(&entry, &variant);
    dbus_message_iter_close_container(dict, &entry);
}

static void append_variant_target_string(DBusMessageIter *dict,
                                         const char *key, const char *value) {
    DBusMessageIter entry;
    DBusMessageIter variant;
    dbus_message_iter_open_container(dict, DBUS_TYPE_DICT_ENTRY, NULL, &entry);
    dbus_message_iter_append_basic(&entry, DBUS_TYPE_STRING, &key);
    dbus_message_iter_open_container(&entry, DBUS_TYPE_VARIANT, "s", &variant);
    dbus_message_iter_append_basic(&variant, DBUS_TYPE_STRING, &value);
    dbus_message_iter_close_container(&entry, &variant);
    dbus_message_iter_close_container(dict, &entry);
}

static void append_item(DBusMessageIter *items, const char *label,
                        const char *action, const char *target,
                        dbus_bool_t separator, const char *link_name,
                        dbus_uint32_t link_group, dbus_uint32_t link_menu) {
    DBusMessageIter attributes;
    const char *type = "separator";
    dbus_message_iter_open_container(items, DBUS_TYPE_ARRAY, "{sv}", &attributes);
    append_variant_string(&attributes, "label", label);
    if (separator) {
        append_variant_string(&attributes, "type", type);
    }
    if (action != NULL) {
        append_variant_string(&attributes, "action", action);
    }
    if (target != NULL) {
        append_variant_target_string(&attributes, "target", target);
    }
    if (link_name != NULL) {
        append_variant_link(&attributes, link_name, link_group, link_menu);
    }
    dbus_message_iter_close_container(items, &attributes);
}

static void append_menu(DBusMessageIter *content, dbus_uint32_t group,
                        dbus_uint32_t menu) {
    DBusMessageIter tuple;
    DBusMessageIter items;

    dbus_message_iter_open_container(content, DBUS_TYPE_STRUCT, NULL, &tuple);
    dbus_message_iter_append_basic(&tuple, DBUS_TYPE_UINT32, &group);
    dbus_message_iter_append_basic(&tuple, DBUS_TYPE_UINT32, &menu);
    dbus_message_iter_open_container(&tuple, DBUS_TYPE_ARRAY, "a{sv}", &items);

    if (group == 0 && menu == 0) {
        append_item(&items, "_File", NULL, NULL, FALSE, ":submenu", 1, 0);
        append_item(&items, "_Edit", NULL, NULL, FALSE, ":submenu", 2, 0);
    } else if (group == 1 && menu == 0) {
        append_item(&items, "_Open", "app.open", "qa-target", FALSE, NULL, 0, 0);
        append_item(&items, "", NULL, NULL, FALSE, ":section", 3, 0);
        append_item(&items, "_Close", "win.close", NULL, FALSE, NULL, 0, 0);
    } else if (group == 2 && menu == 0) {
        append_item(&items, "_Mark", "app.mark", NULL, FALSE, NULL, 0, 0);
        append_item(&items, "", NULL, NULL, FALSE, ":section", 4, 0);
        append_item(&items, "", NULL, NULL, FALSE, ":section", 5, 0);
    } else if (group == 3 && menu == 0) {
        append_item(&items, "_Compact", "app.choose", "compact", FALSE, NULL,
                    0, 0);
        append_item(&items, "_Spacious", "app.choose", "spacious", FALSE, NULL,
                    0, 0);
    } else if (group == 4 && menu == 0) {
        append_item(&items, "_Undo", "win.undo", NULL, FALSE, NULL, 0, 0);
    } else if (group == 5 && menu == 0) {
        append_item(&items, "_Redo", "win.redo", NULL, FALSE, NULL, 0, 0);
    }
    dbus_message_iter_close_container(&tuple, &items);
    dbus_message_iter_close_container(content, &tuple);
}

static int start_requests_group(DBusMessage *call, dbus_uint32_t wanted_group) {
    DBusMessageIter body;
    DBusMessageIter groups;
    if (!dbus_message_iter_init(call, &body) ||
        dbus_message_iter_get_arg_type(&body) != DBUS_TYPE_ARRAY) {
        return 0;
    }
    dbus_message_iter_recurse(&body, &groups);
    while (dbus_message_iter_get_arg_type(&groups) != DBUS_TYPE_INVALID) {
        if (dbus_message_iter_get_arg_type(&groups) != DBUS_TYPE_UINT32) {
            return 0;
        }
        dbus_uint32_t group = 0;
        dbus_message_iter_get_basic(&groups, &group);
        if (group == wanted_group) {
            return 1;
        }
        if (!dbus_message_iter_next(&groups)) {
            break;
        }
    }
    return 0;
}

static DBusMessage *start_reply(DBusMessage *call) {
    DBusMessage *reply = dbus_message_new_method_return(call);
    DBusMessageIter body;
    DBusMessageIter content;

    if (reply == NULL) {
        return NULL;
    }
    dbus_message_iter_init_append(reply, &body);
    dbus_message_iter_open_container(&body, DBUS_TYPE_ARRAY, "(uuaa{sv})",
                                     &content);
    /* Start subscribes to explicit group IDs. Returning only the requested
     * group makes the fixture exercise the client's bounded linked-group
     * fetches instead of masking a missing Start([1, 2]) round-trip. */
    if (start_requests_group(call, 0)) {
        append_menu(&content, 0, 0);
    }
    if (start_requests_group(call, 1)) {
        append_menu(&content, 1, 0);
    }
    if (start_requests_group(call, 2)) {
        append_menu(&content, 2, 0);
    }
    if (start_requests_group(call, 3)) {
        append_menu(&content, 3, 0);
    }
    if (start_requests_group(call, 4)) {
        append_menu(&content, 4, 0);
    }
    if (start_requests_group(call, 5)) {
        append_menu(&content, 5, 0);
    }
    dbus_message_iter_close_container(&body, &content);
    return reply;
}

static void append_action_description(DBusMessageIter *descriptions,
                                      const char *name, dbus_bool_t enabled,
                                      const char *parameter_type,
                                      dbus_bool_t has_state,
                                      dbus_bool_t state_value) {
    DBusMessageIter entry;
    DBusMessageIter description;
    DBusMessageIter state;
    dbus_message_iter_open_container(descriptions, DBUS_TYPE_DICT_ENTRY, NULL,
                                     &entry);
    dbus_message_iter_append_basic(&entry, DBUS_TYPE_STRING, &name);
    dbus_message_iter_open_container(&entry, DBUS_TYPE_STRUCT, NULL,
                                     &description);
    dbus_message_iter_append_basic(&description, DBUS_TYPE_BOOLEAN, &enabled);
    dbus_message_iter_append_basic(&description, DBUS_TYPE_SIGNATURE,
                                   &parameter_type);
    dbus_message_iter_open_container(&description, DBUS_TYPE_ARRAY, "v", &state);
    if (has_state) {
        DBusMessageIter variant;
        dbus_message_iter_open_container(&state, DBUS_TYPE_VARIANT, "b", &variant);
        dbus_message_iter_append_basic(&variant, DBUS_TYPE_BOOLEAN, &state_value);
        dbus_message_iter_close_container(&state, &variant);
    }
    dbus_message_iter_close_container(&description, &state);
    dbus_message_iter_close_container(&entry, &description);
    dbus_message_iter_close_container(descriptions, &entry);
}

static void append_radio_action_description(DBusMessageIter *descriptions,
                                            const char *name,
                                            const char *state_value) {
    DBusMessageIter entry;
    DBusMessageIter description;
    DBusMessageIter state;
    DBusMessageIter variant;
    const dbus_bool_t enabled = TRUE;
    const char *parameter_type = "s";
    dbus_message_iter_open_container(descriptions, DBUS_TYPE_DICT_ENTRY, NULL,
                                     &entry);
    dbus_message_iter_append_basic(&entry, DBUS_TYPE_STRING, &name);
    dbus_message_iter_open_container(&entry, DBUS_TYPE_STRUCT, NULL,
                                     &description);
    dbus_message_iter_append_basic(&description, DBUS_TYPE_BOOLEAN, &enabled);
    dbus_message_iter_append_basic(&description, DBUS_TYPE_SIGNATURE,
                                   &parameter_type);
    dbus_message_iter_open_container(&description, DBUS_TYPE_ARRAY, "v", &state);
    dbus_message_iter_open_container(&state, DBUS_TYPE_VARIANT, "s", &variant);
    dbus_message_iter_append_basic(&variant, DBUS_TYPE_STRING, &state_value);
    dbus_message_iter_close_container(&state, &variant);
    dbus_message_iter_close_container(&description, &state);
    dbus_message_iter_close_container(&entry, &description);
    dbus_message_iter_close_container(descriptions, &entry);
}

static DBusMessage *describe_all_reply(DBusMessage *call, struct Fixture *fixture) {
    DBusMessage *reply = dbus_message_new_method_return(call);
    DBusMessageIter body;
    DBusMessageIter descriptions;

    if (reply == NULL) {
        return NULL;
    }
    dbus_message_iter_init_append(reply, &body);
    dbus_message_iter_open_container(&body, DBUS_TYPE_ARRAY, "{s(bgav)}",
                                     &descriptions);
    append_action_description(&descriptions, "open", TRUE, "s", FALSE, FALSE);
    append_action_description(&descriptions, "mark", fixture->mark_state, "",
                              TRUE, fixture->mark_state);
    append_radio_action_description(&descriptions, "choose", fixture->radio_state);
    dbus_message_iter_close_container(&body, &descriptions);
    FILE *events = fopen(fixture->event_file, "a");
    if (events != NULL) {
        fprintf(events, "described mark=%d radio=%s\n", fixture->mark_state,
                fixture->radio_state);
        fclose(events);
    }
    return reply;
}

static DBusMessage *describe_window_reply(DBusMessage *call) {
    DBusMessage *reply = dbus_message_new_method_return(call);
    DBusMessageIter body;
    DBusMessageIter descriptions;

    if (reply == NULL) {
        return NULL;
    }
    dbus_message_iter_init_append(reply, &body);
    dbus_message_iter_open_container(&body, DBUS_TYPE_ARRAY, "{s(bgav)}",
                                     &descriptions);
    append_action_description(&descriptions, "close", TRUE, "", FALSE, FALSE);
    append_action_description(&descriptions, "undo", TRUE, "", FALSE, FALSE);
    append_action_description(&descriptions, "redo", TRUE, "", FALSE, FALSE);
    dbus_message_iter_close_container(&body, &descriptions);
    return reply;
}

static int parse_activation(DBusMessage *message, char *action,
                            size_t action_size, char *target_value,
                            size_t target_size) {
    DBusMessageIter iter;
    const char *name = NULL;

    if (target_value != NULL && target_size > 0) {
        target_value[0] = '\0';
    }

    if (!dbus_message_iter_init(message, &iter) ||
        dbus_message_iter_get_arg_type(&iter) != DBUS_TYPE_STRING) {
        return 0;
    }
    dbus_message_iter_get_basic(&iter, &name);
    if (name == NULL || !dbus_message_iter_next(&iter) ||
        dbus_message_iter_get_arg_type(&iter) != DBUS_TYPE_ARRAY) {
        return 0;
    }
    DBusMessageIter parameters;
    dbus_message_iter_recurse(&iter, &parameters);
    if (strcmp(name, "open") == 0) {
        if (dbus_message_iter_get_arg_type(&parameters) != DBUS_TYPE_VARIANT) {
            return 0;
        }
        DBusMessageIter target_iter;
        dbus_message_iter_recurse(&parameters, &target_iter);
        if (dbus_message_iter_get_arg_type(&target_iter) != DBUS_TYPE_STRING) {
            return 0;
        }
        const char *target_text = NULL;
        dbus_message_iter_get_basic(&target_iter, &target_text);
        if (target_text == NULL || strcmp(target_text, "qa-target") != 0 ||
            dbus_message_iter_next(&parameters)) {
            return 0;
        }
        snprintf(target_value, target_size, "%s", target_text);
    } else if (strcmp(name, "choose") == 0) {
        if (dbus_message_iter_get_arg_type(&parameters) != DBUS_TYPE_VARIANT) {
            return 0;
        }
        DBusMessageIter target_iter;
        dbus_message_iter_recurse(&parameters, &target_iter);
        if (dbus_message_iter_get_arg_type(&target_iter) != DBUS_TYPE_STRING) {
            return 0;
        }
        const char *target_text = NULL;
        dbus_message_iter_get_basic(&target_iter, &target_text);
        if (target_text == NULL ||
            (strcmp(target_text, "compact") != 0 &&
             strcmp(target_text, "spacious") != 0) ||
            dbus_message_iter_next(&parameters)) {
            return 0;
        }
        snprintf(target_value, target_size, "%s", target_text);
    } else if (dbus_message_iter_get_arg_type(&parameters) != DBUS_TYPE_INVALID) {
        return 0;
    }
    if (!dbus_message_iter_next(&iter) ||
        dbus_message_iter_get_arg_type(&iter) != DBUS_TYPE_ARRAY) {
        return 0;
    }
    snprintf(action, action_size, "%s", name);
    return !dbus_message_iter_next(&iter);
}

static DBusHandlerResult handle_message(DBusConnection *connection,
                                         DBusMessage *message, void *user_data) {
    struct Fixture *fixture = user_data;
    DBusMessage *reply = NULL;
    char action[256];

    if (dbus_message_is_method_call(message, MENUS_INTERFACE, "Start") &&
        strcmp(dbus_message_get_path(message), MENU_PATH) == 0) {
        reply = start_reply(message);
    } else if (dbus_message_is_method_call(message, MENUS_INTERFACE, "End") &&
               strcmp(dbus_message_get_path(message), MENU_PATH) == 0) {
        reply = dbus_message_new_method_return(message);
    } else if (dbus_message_is_method_call(message, ACTIONS_INTERFACE,
                                           "DescribeAll") &&
               strcmp(dbus_message_get_path(message), APPLICATION_PATH) == 0) {
        reply = describe_all_reply(message, fixture);
    } else if (dbus_message_is_method_call(message, ACTIONS_INTERFACE,
                                           "DescribeAll") &&
               strcmp(dbus_message_get_path(message), WINDOW_PATH) == 0) {
        reply = describe_window_reply(message);
    } else if (dbus_message_is_method_call(message, ACTIONS_INTERFACE,
                                           "Activate") &&
               (strcmp(dbus_message_get_path(message), APPLICATION_PATH) == 0 ||
                strcmp(dbus_message_get_path(message), WINDOW_PATH) == 0)) {
        char target[64];
        if (parse_activation(message, action, sizeof(action), target,
                             sizeof(target))) {
            const char *path = dbus_message_get_path(message);
            if ((strcmp(path, APPLICATION_PATH) == 0 &&
                 strcmp(action, "open") != 0 && strcmp(action, "mark") != 0 &&
                 strcmp(action, "choose") != 0) ||
                (strcmp(path, WINDOW_PATH) == 0 &&
                 strcmp(action, "close") != 0 && strcmp(action, "undo") != 0 &&
                 strcmp(action, "redo") != 0)) {
                reply = dbus_message_new_error(message, DBUS_ERROR_INVALID_ARGS,
                                                "Action routed to wrong object path");
                goto send_reply;
            }
            if (strcmp(action, "mark") == 0) {
                fixture->mark_state = !fixture->mark_state;
            } else if (strcmp(action, "choose") == 0 && target[0] != '\0') {
                if (strcmp(target, "compact") == 0) {
                    strcpy(fixture->radio_state, "compact");
                } else {
                    strcpy(fixture->radio_state, "spacious");
                }
            }
            FILE *events = fopen(fixture->event_file, "a");
            if (events != NULL) {
                fprintf(events, "activated action=%s target=%s\n", action,
                        target);
                fclose(events);
            }
            reply = dbus_message_new_method_return(message);
        } else {
            reply = dbus_message_new_error(message, DBUS_ERROR_INVALID_ARGS,
                                            "Expected org.gtk.Actions Activate");
        }
    }
send_reply:
    if (reply == NULL) {
        return DBUS_HANDLER_RESULT_NOT_YET_HANDLED;
    }
    if (!dbus_connection_send(connection, reply, NULL)) {
        dbus_message_unref(reply);
        return DBUS_HANDLER_RESULT_NEED_MEMORY;
    }
    dbus_connection_flush(connection);
    dbus_message_unref(reply);
    return DBUS_HANDLER_RESULT_HANDLED;
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
    fixture.mark_state = TRUE;
    snprintf(fixture.radio_state, sizeof(fixture.radio_state), "%s", "compact");
    dbus_error_init(&error);
    connection = dbus_bus_get(DBUS_BUS_SESSION, &error);
    if (connection == NULL) {
        fail_dbus("connect session bus", &error);
    }
    dbus_connection_set_exit_on_disconnect(connection, FALSE);
    if (!dbus_connection_add_filter(connection, handle_message, &fixture, NULL)) {
        fprintf(stderr, "qa-gmenu-exporter: cannot install message filter\n");
        return 1;
    }
    unique_name = dbus_bus_get_unique_name(connection);
    if (unique_name == NULL || unique_name[0] != ':') {
        fprintf(stderr, "qa-gmenu-exporter: no unique session bus name\n");
        return 1;
    }
    bus = fopen(fixture.bus_file, "w");
    if (bus == NULL) {
        fprintf(stderr, "qa-gmenu-exporter: %s: %s\n", fixture.bus_file,
                strerror(errno));
        return 1;
    }
    fprintf(bus, "%s\n", unique_name);
    fclose(bus);
    while (dbus_connection_read_write_dispatch(connection, -1)) {
    }
    dbus_connection_remove_filter(connection, handle_message, &fixture);
    dbus_connection_unref(connection);
    return 0;
}

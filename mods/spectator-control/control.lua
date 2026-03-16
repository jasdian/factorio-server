-- spectator-control: permission-based spectator enforcement
-- Players joining are spectators by default (can look, can't interact).
-- The backend promotes registered players via RCON remote.call.

local SPECTATOR = "Spectators"

local RESTRICTED_ACTIONS = {
    -- Building
    "build", "build_rail", "build_terrain", "stop_drag_build",
    -- Mining
    "begin_mining", "begin_mining_terrain",
    -- Crafting
    "craft", "cancel_craft",
    -- Deconstruction / upgrade
    "deconstruct", "cancel_deconstruct", "upgrade", "cancel_upgrade",
    -- Item manipulation
    "drop_item", "destroy_item", "destroy_opened_item",
    "cursor_split", "cursor_transfer",
    "inventory_split", "inventory_transfer",
    "fast_entity_transfer", "fast_entity_split",
    "stack_split", "stack_transfer",
    "send_stack_to_trash", "send_stacks_to_trash",
    "trash_not_requested_items",
    -- Equipment
    "place_equipment", "take_equipment",
    -- Entity interaction
    "rotate_entity", "flip_entity",
    "paste_entity_settings", "copy_entity_settings",
    "use_item", "start_repair",
    -- Wires
    "wire_dragging", "remove_cables",
    -- Area selection tools
    "select_area", "alt_select_area",
    "reverse_select_area", "alt_reverse_select_area",
    -- Assembling / production
    "setup_assembling_machine", "reset_assembling_machine",
    "flush_opened_entity_fluid", "flush_opened_entity_specific_fluid",
    -- Vehicles
    "toggle_driving", "send_spidertron",
    -- Rockets / space
    "launch_rocket", "create_space_platform", "delete_space_platform",
    -- Rolling stock
    "connect_rolling_stock", "disconnect_rolling_stock",
    -- Blueprint setup
    "setup_blueprint",
    -- Combat
    "change_shooting_state",
    -- Market
    "market_offer",
}

local function ensure_spectator_group()
    local group = game.permissions.get_group(SPECTATOR)
    if not group then
        group = game.permissions.create_group(SPECTATOR)
    end
    for _, name in pairs(RESTRICTED_ACTIONS) do
        local action = defines.input_action[name]
        if action then
            group.set_allows_action(action, false)
        end
    end
    return group
end

local function init()
    ensure_spectator_group()
    if not storage.players then
        storage.players = {} -- { [name] = "player" | "spectator" }
    end
end

script.on_init(init)
script.on_configuration_changed(function() ensure_spectator_group() end)

local function assign_group(player)
    local role = storage.players[player.name]
    if role == "player" then
        local default = game.permissions.get_group("Default")
        if default then player.permission_group = default end
    else
        -- Unknown or spectator → restrict
        local group = game.permissions.get_group(SPECTATOR)
        if group then player.permission_group = group end
    end
end

script.on_event(defines.events.on_player_joined_game, function(event)
    local player = game.get_player(event.player_index)
    if player then assign_group(player) end
end)

-- Remote interface called by the backend via RCON:
--   /silent-command remote.call('spectator_control', 'add_spectator', 'Name')
--   /silent-command remote.call('spectator_control', 'remove_spectator', 'Name')
--   /silent-command remote.call('spectator_control', 'add_player', 'Name')
remote.add_interface("spectator_control", {
    add_spectator = function(name)
        storage.players[name] = "spectator"
        local player = game.get_player(name)
        if player and player.connected then
            local group = game.permissions.get_group(SPECTATOR)
            if group then player.permission_group = group end
        end
        rcon.print("ok")
    end,

    remove_spectator = function(name)
        storage.players[name] = "player"
        local player = game.get_player(name)
        if player and player.connected then
            local default = game.permissions.get_group("Default")
            if default then player.permission_group = default end
        end
        rcon.print("ok")
    end,

    add_player = function(name)
        storage.players[name] = "player"
        local player = game.get_player(name)
        if player and player.connected then
            local default = game.permissions.get_group("Default")
            if default then player.permission_group = default end
        end
        rcon.print("ok")
    end,

    get_role = function(name)
        rcon.print(storage.players[name] or "unknown")
    end,

    list_players = function()
        local parts = {}
        for n, role in pairs(storage.players) do
            parts[#parts + 1] = n .. "=" .. role
        end
        rcon.print(table.concat(parts, ","))
    end,

    reset = function()
        storage.players = {}
        -- Move everyone to spectator group
        local group = game.permissions.get_group(SPECTATOR)
        if group then
            for _, player in pairs(game.connected_players) do
                player.permission_group = group
            end
        end
        rcon.print("ok")
    end,
})

local function write_production()
    local t = {}
    for _, surface in pairs(game.surfaces) do
        for _, ty in pairs({"assembling-machine", "furnace"}) do
            for _, entity in pairs(surface.find_entities_filtered{type = ty}) do
                t[entity.unit_number] = entity.products_finished
            end
        end
    end
    helpers.write_file('production-' .. game.tick .. '.json', helpers.table_to_json(t))
end

local tile_size = 256
local zoom = 4

local function to_idx_n(v) return math.floor(v / tile_size) end
local function to_idx(x, y) return to_idx_n(x) .. "_" .. to_idx_n(y) end
local function from_idx(idx)
    local sep = string.find(idx, "_")
    local x = tonumber(string.sub(idx, 1, sep - 1))
    local y = tonumber(string.sub(idx, sep + 1))
    return x, y
end

local function write_assemblers()
    local t = {}
    -- needs to be per surface
    local xys = {}
    local recps = {}
    for _, surface in pairs(game.surfaces) do
        for _, ty in pairs({"assembling-machine", "furnace"}) do
            for _, entity in pairs(surface.find_entities_filtered{type = ty}) do
                local recp = entity.get_recipe()
                local recipe_name = nil
                if recp ~= nil then
                    recipe_name = recp.name
                    if recps[recipe_name] == nil then
                        recps[recipe_name] = {
                            ingredients = recp.ingredients,
                            products = recp.products,
                        }
                    end
                end

                local as_idx = to_idx(entity.position.x, entity.position.y)
                xys[as_idx] = entity.unit_number
                t[entity.unit_number] = {
                    surface = surface.name,
                    type = entity.type,
                    name = entity.name,
                    position = { entity.position.x, entity.position.y },
                    recipe = recipe_name,
                    products_finished = entity.products_finished,
                    direction = entity.direction,
                }
            end
        end

        for idx, _ in pairs(xys) do
            local x, y = from_idx(idx)
            local center_x = (x + 0.5) * tile_size
            local center_y = (y + 0.5) * tile_size

            game.take_screenshot({
                path="assemblers-" .. surface.name .. "-" .. idx .. ".png",
                position={center_x, center_y},
                -- 32 is the magic built in factorio tile size coefficient
                zoom=zoom/32,
                surface=surface,
                resolution={tile_size * zoom, tile_size * zoom},
                water_tick=0, show_entity_info=true, daytime=1, hide_clouds=true,
            })
        end
    end

    helpers.write_file('assemblers.json', helpers.table_to_json(
        { t=t, xys=xys, tick=game.tick, recps=recps }))
end

script.on_nth_tick(60 * 15 + 7, write_production)
commands.add_command("write-screenshots", nil, write_assemblers)

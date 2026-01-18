local function write_production()
    local t = {}
    for _, surface in pairs(game.surfaces) do
        for _, ty in pairs({"assembling-machine", "furnace"}) do
            for _, entity in pairs(surface.find_entities_filtered{type = ty}) do
                t[entity.unit_number] = entity.products_finished
            end
        end
    end
    helpers.write_file('production.json', helpers.table_to_json(t))
end

local function write_assemblers()
    local t = {}
    for _, surface in pairs(game.surfaces) do
        local xs = {}
        local ys = {}
        for _, ty in pairs({"assembling-machine", "furnace"}) do
            for _, entity in pairs(surface.find_entities_filtered{type = ty}) do
                table.insert(xs, entity.position.x)
                table.insert(ys, entity.position.y)
                t[entity.unit_number] = {
                    name = entity.name,
                    type = entity.type,
                    position = entity.position,
                    recipe = entity.get_recipe() and entity.get_recipe().name or nil,
                    finished = entity.products_finished,
                    surface = surface.name,
                }
            end
        end
        table.sort(xs)
        table.sort(ys)
        local xrange = xs[#xs] - xs[1]
        local yrange = ys[#ys] - ys[1]
        local center_x = (xs[#xs] + xs[1]) / 2
        local center_y = (ys[#ys] + ys[1]) / 2
        local scale_factor = 8000
        local zoom = math.max(xrange / scale_factor, yrange / scale_factor)
        game.print("screenshot: " .. xrange .. "x" .. yrange .. " zoom: " .. zoom .. "center: " .. center_x .. "," .. center_y)
        game.take_screenshot({
            path="assemblers-" .. surface.name .. ".png",
            position={center_x, center_y},
            zoom=zoom,
            surface=surface,
            resolution={7680,4320}, water_tick=0, show_entity_info=true, daytime=1, hide_clouds=true,
         })
    end
    helpers.write_file('assemblers.json', helpers.table_to_json({ t }))
end

script.on_nth_tick(60 * 5, write_production)
script.on_nth_tick(60 * 30, write_assemblers)

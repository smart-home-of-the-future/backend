fn on_msg(ctx, dev, channel, data) {
    ctx.log("received some data from " + dev + ": " + data);
}

#{
    on_msg: on_msg,
    only_channel: ["is_moving"]
}
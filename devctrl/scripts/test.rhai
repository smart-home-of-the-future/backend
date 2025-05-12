fn on_msg(ctx, dev, channel, data) {
    ctx.log(`received some data from ${dev} on ${channel}: ${data}`);
}

#{
    on_msg: on_msg,
    //only_channels: ["ch1"]
}

use std::{marker::PhantomData, ops::Deref};

use bevy::{
    app::{App, Plugin, Update},
    ecs::{
        event::Event,
        message::{Message, MessageWriter},
        observer::On,
        resource::Resource,
        system::Res,
    },
    tasks::IoTaskPool,
};

use crossbeam_channel::{Receiver, Sender};
pub use ehttp::Headers;
pub use ehttp::Method;
use serde::{Serialize, de::DeserializeOwned};

pub mod prelude {
    pub use crate::{AppExt as _, HttpPlugin, HttpRequest, HttpResponse, RequestType};
    pub use ehttp::Method;
}

pub trait RequestType: Serialize + Send + Sync + 'static {
    type Extra: Send + Sync;
    type Response: DeserializeOwned;
    const METHOD: Method;

    fn headers(&self, headers: &mut Headers) {
        let _ = headers;
    }

    fn extra(&self) -> Self::Extra;

    fn endpoint<'r>(&'r self) -> impl ToString;
}

#[derive(Event)]
pub struct HttpRequest<T>(pub T);

#[derive(Message)]
pub struct HttpResponse<T: RequestType> {
    response: ehttp::Response,
    extra: T::Extra,
    phantom: PhantomData<T>,
}

impl<T: RequestType> Deref for HttpResponse<T> {
    type Target = ehttp::Response;

    fn deref(&self) -> &Self::Target {
        &self.response
    }
}

impl<T: RequestType> HttpResponse<T> {
    pub fn json(&self) -> Result<T::Response, serde_json::Error> {
        self.response.json()
    }

    pub fn extra(&self) -> &T::Extra {
        &self.extra
    }
}

pub struct HttpResult<T>(pub Result<(ehttp::Response, T), Error>);

#[derive(Message, Debug)]
pub enum Error {
    Serde(serde_json::Error),
    Ehttp(String),
}

#[derive(Resource)]
struct Channel<T: RequestType>(
    Sender<HttpResult<T::Extra>>,
    Receiver<HttpResult<T::Extra>>,
    PhantomData<T>,
);

fn on_request<T: RequestType>(ev: On<HttpRequest<T>>, channel: Res<Channel<T>>) {
    let pool = IoTaskPool::get();
    let sender = channel.0.clone();
    let extra = ev.0.extra();
    let body = match serde_json::to_vec(&ev.0) {
        Ok(body) => body,
        Err(err) => {
            sender.send(HttpResult(Err(Error::Serde(err)))).unwrap();
            return;
        }
    };
    let mut req = ehttp::Request::new(
        T::METHOD,
        ev.0.endpoint(),
        Headers::new(&[("Accept", "*/*"), ("Content-Type", "application/json")]),
    )
    .with_body(body);
    ev.0.headers(&mut req.headers);

    pool.spawn(async move {
        let res = match ehttp::fetch_async(req).await {
            Ok(v) => v,
            Err(err) => {
                sender.send(HttpResult(Err(Error::Ehttp(err)))).unwrap();
                return;
            }
        };

        sender.send(HttpResult(Ok((res, extra)))).unwrap();
    })
    .detach();
}

fn on_response<T: RequestType>(
    channel: Res<Channel<T>>,
    mut ok_writer: MessageWriter<HttpResponse<T>>,
    mut err_writer: MessageWriter<Error>,
) {
    for res in channel.1.try_iter() {
        match res.0 {
            Ok(v) => {
                ok_writer.write(HttpResponse {
                    response: v.0,
                    extra: v.1,
                    phantom: PhantomData,
                });
            }
            Err(err) => {
                err_writer.write(err);
            }
        }
    }
}

/// Uses triggers for requests, messages for responses.
pub struct HttpPlugin;

impl Plugin for HttpPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<Error>();
    }
}

pub trait AppExt {
    fn add_request_type<T: RequestType>(&mut self) -> &mut Self;
}

impl AppExt for App {
    fn add_request_type<T: RequestType>(&mut self) -> &mut Self {
        let (sender, recv) = crossbeam_channel::unbounded();
        self.add_observer(on_request::<T>)
            .add_systems(Update, on_response::<T>)
            .insert_resource(Channel::<T>(sender, recv, PhantomData))
            .add_message::<HttpResponse<T>>()
    }
}

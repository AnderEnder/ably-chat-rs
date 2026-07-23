# \MessagesApi

All URIs are relative to *https://rest.ably.io*

Method | HTTP request | Description
------------- | ------------- | -------------
[**delete_message**](MessagesApi.md#delete_message) | **POST** /chat/v4/rooms/{roomName}/messages/{serial}/delete | Delete a message
[**get_message**](MessagesApi.md#get_message) | **GET** /chat/v4/rooms/{roomName}/messages/{serial} | Get a single message
[**get_message_versions**](MessagesApi.md#get_message_versions) | **GET** /chat/v4/rooms/{roomName}/messages/{serial}/versions | Get message versions
[**get_messages**](MessagesApi.md#get_messages) | **GET** /chat/v4/rooms/{roomName}/messages | Get message history
[**send_message**](MessagesApi.md#send_message) | **POST** /chat/v4/rooms/{roomName}/messages | Send a message
[**update_message**](MessagesApi.md#update_message) | **PUT** /chat/v4/rooms/{roomName}/messages/{serial} | Update (edit) a message



## delete_message

> models::Message delete_message(room_name, serial, x_ably_version, idempotency_key, delete_message_request)
Delete a message

Soft-deletes a message. This is a `POST` to a `/delete` sub-resource (not an HTTP `DELETE`). Produces a new message version with action `message.delete`; the deleted message remains retrievable with its delete action applied. 

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**room_name** | **String** | The name of the chat room. Sent URL-encoded. | [required] |
**serial** | **String** | The unique serial identifier of the message. Sent URL-encoded. | [required] |
**x_ably_version** | Option<**String**> | The Ably API version. The Chat SDK sends `4`. May alternatively be supplied as the `?v=` query parameter.  |  |[default to 4]
**idempotency_key** | Option<**String**> | Optional idempotency key for safely retrying a create/update/delete without risking duplicate operations. The SDK generates one automatically when idempotent REST publishing is enabled.  |  |
**delete_message_request** | Option<[**DeleteMessageRequest**](DeleteMessageRequest.md)> |  |  |

### Return type

[**models::Message**](Message.md)

### Authorization

[basicKey](../README.md#basicKey), [bearerToken](../README.md#bearerToken)

### HTTP request headers

- **Content-Type**: application/json
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## get_message

> models::Message get_message(room_name, serial, x_ably_version)
Get a single message

Returns the latest version of a single message by its serial.

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**room_name** | **String** | The name of the chat room. Sent URL-encoded. | [required] |
**serial** | **String** | The unique serial identifier of the message. Sent URL-encoded. | [required] |
**x_ably_version** | Option<**String**> | The Ably API version. The Chat SDK sends `4`. May alternatively be supplied as the `?v=` query parameter.  |  |[default to 4]

### Return type

[**models::Message**](Message.md)

### Authorization

[basicKey](../README.md#basicKey), [bearerToken](../README.md#bearerToken)

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## get_message_versions

> Vec<models::Message> get_message_versions(room_name, serial, x_ably_version)
Get message versions

Returns a paginated list of all versions (create, updates, deletes) of a message. The response body is a JSON array; pagination is conveyed via RFC 5988 `Link` response headers. 

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**room_name** | **String** | The name of the chat room. Sent URL-encoded. | [required] |
**serial** | **String** | The unique serial identifier of the message. Sent URL-encoded. | [required] |
**x_ably_version** | Option<**String**> | The Ably API version. The Chat SDK sends `4`. May alternatively be supplied as the `?v=` query parameter.  |  |[default to 4]

### Return type

[**Vec<models::Message>**](Message.md)

### Authorization

[basicKey](../README.md#basicKey), [bearerToken](../README.md#bearerToken)

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## get_messages

> Vec<models::Message> get_messages(room_name, x_ably_version, start, end, direction, limit, from_serial)
Get message history

Returns a paginated list of messages for a room, ordered per the `direction` parameter. The response body is a JSON array of messages; pagination is conveyed via RFC 5988 `Link` response headers (`rel=\"first\"`, `rel=\"current\"`, `rel=\"next\"`). 

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**room_name** | **String** | The name of the chat room. Sent URL-encoded. | [required] |
**x_ably_version** | Option<**String**> | The Ably API version. The Chat SDK sends `4`. May alternatively be supplied as the `?v=` query parameter.  |  |[default to 4]
**start** | Option<**i64**> | Earliest message timestamp to include, as milliseconds since the Unix epoch. Messages with a timestamp greater than or equal to this value are returned. Defaults to the beginning of time.  |  |
**end** | Option<**i64**> | Latest message timestamp to include, as milliseconds since the Unix epoch. Messages with a timestamp strictly less than this value are returned. Defaults to now.  |  |
**direction** | Option<**String**> | Order in which to return messages. `backwards` returns newest first; `forwards` returns oldest first.  |  |[default to backwards]
**limit** | Option<**i32**> | Maximum number of messages to return in a single page. |  |[default to 100]
**from_serial** | Option<**String**> | Serial indicating the starting point for message retrieval. This serial is specific to the region of the channel the client is connected to.  |  |

### Return type

[**Vec<models::Message>**](Message.md)

### Authorization

[basicKey](../README.md#basicKey), [bearerToken](../README.md#bearerToken)

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## send_message

> models::Message send_message(room_name, send_message_request, x_ably_version, idempotency_key)
Send a message

Publishes a new message to the room.

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**room_name** | **String** | The name of the chat room. Sent URL-encoded. | [required] |
**send_message_request** | [**SendMessageRequest**](SendMessageRequest.md) |  | [required] |
**x_ably_version** | Option<**String**> | The Ably API version. The Chat SDK sends `4`. May alternatively be supplied as the `?v=` query parameter.  |  |[default to 4]
**idempotency_key** | Option<**String**> | Optional idempotency key for safely retrying a create/update/delete without risking duplicate operations. The SDK generates one automatically when idempotent REST publishing is enabled.  |  |

### Return type

[**models::Message**](Message.md)

### Authorization

[basicKey](../README.md#basicKey), [bearerToken](../README.md#bearerToken)

### HTTP request headers

- **Content-Type**: application/json
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## update_message

> models::Message update_message(room_name, serial, update_message_request, x_ably_version, idempotency_key)
Update (edit) a message

Updates a message. All fields under `message` are replaced; any omitted `message` field is reset to empty. Produces a new message version with action `message.update`. 

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**room_name** | **String** | The name of the chat room. Sent URL-encoded. | [required] |
**serial** | **String** | The unique serial identifier of the message. Sent URL-encoded. | [required] |
**update_message_request** | [**UpdateMessageRequest**](UpdateMessageRequest.md) |  | [required] |
**x_ably_version** | Option<**String**> | The Ably API version. The Chat SDK sends `4`. May alternatively be supplied as the `?v=` query parameter.  |  |[default to 4]
**idempotency_key** | Option<**String**> | Optional idempotency key for safely retrying a create/update/delete without risking duplicate operations. The SDK generates one automatically when idempotent REST publishing is enabled.  |  |

### Return type

[**models::Message**](Message.md)

### Authorization

[basicKey](../README.md#basicKey), [bearerToken](../README.md#bearerToken)

### HTTP request headers

- **Content-Type**: application/json
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)

